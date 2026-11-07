//! Admission and direct iterative source walk. No invoice XML parser is called.
use crate::{
    budget::{mul, Control, Failure},
    capture::Sink,
    model::*,
    SourceDocument,
};
use einvoice_source::{Children, Encoding, Node, NodeKind};
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::{
    validation::{SchemaValidator, SchemaValidity, ValidationRuntime},
    SchemaSet,
};

fn name(n: &einvoice_source::Name, c: &Control<'_>) -> Result<(), Failure> {
    c.cap("name_bytes", n.local().len(), c.limits.name_bytes)?;
    c.cap(
        "name_bytes",
        n.prefix().map_or(0, str::len),
        c.limits.name_bytes,
    )?;
    c.cap("uri_bytes", n.namespace_uri().len(), c.limits.uri_bytes)?;
    c.work(n.local().len())?;
    c.work(n.prefix().map_or(0, str::len))?;
    c.work(n.namespace_uri().len())
}
/// Inspect actual inventory: SourceDocument currently exports no parse-limit
/// certificate. Complete this pass before any instance string can be interned.
fn admit(c: &Control<'_>) -> Result<(), Failure> {
    let l = c.limits;
    l.check()?;
    c.cap(
        "input_bytes",
        c.source.original_bytes().len(),
        l.input_bytes,
    )?;
    c.cap("nodes", c.source.nodes().len(), l.nodes)?;
    c.work(c.source.original_bytes().len())?;
    c.work(c.source.source_view().len())?;
    for node in c.source.nodes() {
        c.work(1)?;
        if let Some(n) = node.name() {
            name(n, c)?;
        }
        if let Some(value) = node.content() {
            c.cap("value_bytes", value.len(), l.value_bytes)?;
            c.work(value.len())?;
        }
        if node.kind() != NodeKind::Element {
            continue;
        }
        let mut ancestor = Some(node);
        let mut depth = 0usize;
        let mut declarations = 0usize;
        let mut namespace_scan_bytes = 0usize;
        while let Some(n) = ancestor {
            c.work(1)?;
            if n.kind() == NodeKind::Element {
                depth = depth.checked_add(1).ok_or(Failure::ArithmeticOverflow)?;
            }
            for d in n.declarations() {
                c.work(1)?;
                declarations = declarations
                    .checked_add(1)
                    .ok_or(Failure::ArithmeticOverflow)?;
                namespace_scan_bytes = namespace_scan_bytes
                    .checked_add(d.prefix().len())
                    .ok_or(Failure::ArithmeticOverflow)?;
            }
            ancestor = n.parent();
        }
        c.cap("depth", depth, l.depth)?;
        c.cap(
            "attributes_per_element",
            node.attributes().len(),
            l.attributes_per_element,
        )?;
        c.cap(
            "declarations_per_element",
            node.declarations().len(),
            l.declarations_per_element,
        )?;
        for d in node.declarations() {
            c.cap("name_bytes", d.prefix().len(), l.name_bytes)?;
            c.cap("uri_bytes", d.uri().len(), l.uri_bytes)?;
            c.work(d.prefix().len())?;
            c.work(d.uri().len())?;
        }
        // Namespace iteration resolves each ancestor declaration by scanning
        // ancestor frames. Reserve a conservative bound before iterator work,
        // including prefix comparisons and the implicit xml lookup. Reserve
        // twice: inventory pass plus later execution snapshot.
        let scans = declarations
            .checked_add(1)
            .ok_or(Failure::ArithmeticOverflow)?;
        let unit = namespace_scan_bytes
            .checked_add(scans)
            .and_then(|n| n.checked_add(depth))
            .ok_or(Failure::ArithmeticOverflow)?;
        c.work(mul(mul(scans, unit)?, 2)?)?;
        let mut effective = 0usize;
        for ns in node.namespaces() {
            c.cap("name_bytes", ns.prefix().len(), l.name_bytes)?;
            c.cap("uri_bytes", ns.uri().len(), l.uri_bytes)?;
            effective = effective
                .checked_add(1)
                .ok_or(Failure::ArithmeticOverflow)?;
            c.cap(
                "namespaces_per_element",
                effective,
                l.namespaces_per_element,
            )?;
            c.work(ns.prefix().len())?;
            c.work(ns.uri().len())?;
        }
        c.cap(
            "namespace_temporary_bytes",
            mul(effective, std::mem::size_of::<(usize, usize)>())?,
            l.temporary_bytes,
        )?;
        for a in node.attributes() {
            c.work(1)?;
            name(a.name(), c)?;
            c.cap("value_bytes", a.value().len(), l.value_bytes)?;
            c.work(a.value().len())?;
        }
    }
    c.poll()
}
fn check(runtime: &ValidationRuntime<'_, Sink<'_, '_>>, c: &Control<'_>) -> Result<(), Failure> {
    c.poll()?;
    if runtime.operational_failure().is_some() {
        return c.fail(Failure::Backend {
            phase: "operational",
        });
    }
    Ok(())
}
fn open(
    node: Node<'_>,
    schema: &SchemaSet,
    r: &mut ValidationRuntime<'_, Sink<'_, '_>>,
    c: &Control<'_>,
) -> Result<(), Failure> {
    c.work(1)?;
    check(r, c)?;
    let mut ns = NamespaceContextSnapshot::default();
    // Admission already bounded the effective count and worst-case axis work.
    // Charge each string before backend interning and reserve before push.
    for binding in node.namespaces() {
        c.work(1)?;
        c.work(binding.prefix().len())?;
        c.work(binding.uri().len())?;
        if binding.prefix() == "xml" {
            continue;
        }
        if binding.prefix().is_empty() {
            ns.default_ns = Some(schema.name_table.add(binding.uri()));
        } else {
            ns.bindings
                .try_reserve(1)
                .map_err(|_| Failure::Allocation)?;
            ns.bindings.push((
                schema.name_table.add(binding.prefix()),
                schema.name_table.add(binding.uri()),
            ));
        }
    }
    let mut xsi_type = None;
    let mut xsi_nil = None;
    for a in node.attributes() {
        c.work(1)?;
        if a.name().namespace_uri() == "http://www.w3.org/2001/XMLSchema-instance" {
            match a.name().local() {
                "type" => xsi_type = Some(a.value()),
                "nil" => xsi_nil = Some(a.value()),
                _ => {}
            }
        }
    }
    let n = node.name().expect("source element");
    name(n, c)?;
    c.work(1)?;
    r.validate_element_novalue(n.local(), n.namespace_uri(), xsi_type, xsi_nil, &ns);
    check(r, c)?;
    for a in node.attributes() {
        name(a.name(), c)?;
        c.work(a.value().len())?;
        c.work(1)?;
        r.validate_attribute(a.name().local(), a.name().namespace_uri(), a.value());
        check(r, c)?;
    }
    c.work(1)?;
    r.validate_end_of_attributes_novalue();
    check(r, c)
}
fn walk(
    schema: &SchemaSet,
    r: &mut ValidationRuntime<'_, Sink<'_, '_>>,
    c: &Control<'_>,
) -> Result<RootValidity, Failure> {
    let root = c.source.root_element();
    open(root, schema, r, c)?;
    let mut stack: Vec<Children<'_>> = Vec::new();
    c.cap(
        "walker_temporary_bytes",
        std::mem::size_of::<Children<'_>>(),
        c.limits.temporary_bytes,
    )?;
    stack.try_reserve(1).map_err(|_| Failure::Allocation)?;
    stack.push(root.children());
    loop {
        c.work(1)?;
        check(r, c)?;
        match stack.last_mut().expect("open root").next() {
            Some(n) if n.kind() == NodeKind::Element => {
                c.cap(
                    "walker_temporary_bytes",
                    mul(
                        stack
                            .len()
                            .checked_add(1)
                            .ok_or(Failure::ArithmeticOverflow)?,
                        std::mem::size_of::<Children<'_>>(),
                    )?,
                    c.limits.temporary_bytes,
                )?;
                stack.try_reserve(1).map_err(|_| Failure::Allocation)?;
                open(n, schema, r, c)?;
                stack.push(n.children());
            }
            Some(n) if n.kind() == NodeKind::Text => {
                let value = n.content().expect("source text");
                c.work(value.len())?;
                if value
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                {
                    r.validate_whitespace(value);
                } else {
                    r.validate_text(value);
                }
                check(r, c)?;
            }
            Some(_) => {} // Comments and PI remain in source, not XSD character data.
            None => {
                c.work(1)?;
                let validity = r.validate_end_element_novalue();
                check(r, c)?;
                stack.pop();
                if stack.is_empty() {
                    return Ok(match validity {
                        SchemaValidity::Valid => RootValidity::Valid,
                        SchemaValidity::Invalid => RootValidity::Invalid,
                        SchemaValidity::NotKnown => RootValidity::NotKnown,
                    });
                }
            }
        }
    }
}
fn encoding(source: &SourceDocument) -> &'static str {
    match source.encoding() {
        Encoding::Utf8 => "UTF-8",
        Encoding::Utf16Le => "UTF-16LE",
        Encoding::Utf16Be => "UTF-16BE",
        Encoding::Latin1 => "ISO-8859-1",
        Encoding::Ascii => "US-ASCII",
    }
}
pub(crate) fn run(
    source: &SourceDocument,
    limits: &crate::RequestLimits,
    signals: crate::RequestControl<'_>,
    schema: &SchemaSet,
    validator: &SchemaValidator<'_>,
) -> Observation {
    let c = Control {
        source,
        limits,
        signals,
        state: Default::default(),
    };
    let mut observation = Observation {
        input_sha256: None,
        input_bytes: source.original_bytes().len(),
        encoding: encoding(source),
        bom_bytes: source.bom_bytes(),
        validated_root: source.root_element().covering_span().into(),
        validated_root_reference: None,
        assessment: Assessment::Incomplete,
        root_validity: None,
        events: Vec::new(),
        failure: None,
        finalization_attempted: false,
        finalization_succeeded: false,
        finalization_error: None,
        operational_failure: None,
        terminal_capture_failure: None,
        work: 0,
        retained_diagnostic_bytes: 0,
        observed_callbacks: 0,
    };
    let admitted = limits
        .check()
        .and_then(|()| {
            c.cap(
                "input_bytes",
                source.original_bytes().len(),
                limits.input_bytes,
            )
        })
        .and_then(|()| c.work(source.original_bytes().len()))
        .map(|()| {
            observation.input_sha256 = Some(crate::sha256(source.original_bytes()));
        })
        .and_then(|()| crate::capture::preorder(source.root_element(), &c))
        .map(|preorder| {
            observation.validated_root_reference = Some(SourceReference::Node { preorder })
        })
        .and_then(|()| admit(&c));
    let result = admitted.and_then(|()| {
        let mut runtime = validator.start_run(Sink(&c));
        let walked = walk(schema, &mut runtime, &c);
        if let Ok(root) = walked {
            observation.root_validity = Some(root);
        }
        let before_final = c.state.borrow().errors;
        let mut terminal_remaining = limits.terminal_bytes;
        let result = walked.and_then(|_| c.work(1)).and_then(|()| {
            observation.finalization_attempted = true;
            let ended = runtime.end_validation();
            observation.finalization_succeeded = ended.is_ok();
            if let Err(e) = ended {
                match crate::capture::terminal(&e, &c, &mut terminal_remaining) {
                    Ok(d) => observation.finalization_error = Some(d),
                    Err(f) => observation.terminal_capture_failure = Some(f),
                }
                let failure = Failure::Backend {
                    phase: "finalization",
                };
                return c.fail(failure);
            }
            check(&runtime, &c)
        });
        if let Err(e) = &result {
            let _ = c.fail::<()>(e.clone());
            if !observation.finalization_attempted {
                runtime.abort_run("source adapter stopped");
            }
        }
        if let Some(e) = runtime.operational_failure() {
            match crate::capture::terminal(e, &c, &mut terminal_remaining) {
                Ok(d) => observation.operational_failure = Some(d),
                Err(f) => {
                    observation.terminal_capture_failure.get_or_insert(f);
                }
            }
        }
        result?;
        match (
            observation.root_validity,
            before_final,
            c.state.borrow().errors,
        ) {
            (Some(RootValidity::Valid), 0, 0) => observation.assessment = Assessment::Valid,
            (Some(RootValidity::Valid), 0, _) | (Some(RootValidity::Invalid), _, _) => {
                observation.assessment = Assessment::Invalid
            }
            _ => return Err(Failure::Consistency),
        }
        Ok(())
    });
    if let Err(f) = result {
        let _ = c.fail::<()>(f);
        observation.assessment = Assessment::Incomplete;
    }
    let state = c.state.into_inner();
    observation.failure = state.failure;
    observation.events = state.events;
    observation.work = state.work;
    observation.retained_diagnostic_bytes = state.bytes;
    observation.observed_callbacks = state.callbacks;
    observation
}
