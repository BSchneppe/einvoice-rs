//! Single ordered sink; field enumeration precedes projection/copying.
use crate::{
    budget::{mul, Control, Failure, Size},
    evidence::NativeCause,
    model::*,
};
use xsd_schema::parser::location::SourceLocation;
use xsd_schema::validation::{errors as b, ValidationError, ValidationSink, ValidationWarning};

struct Fields<'a> {
    code: &'a str,
    message: &'a str,
    location: Option<&'a SourceLocation>,
    subject: Option<&'a b::ValidationSubject>,
    path: Option<&'a b::InstancePath>,
    element_path: Option<&'a str>,
    evidence: Option<&'a b::DiagnosticEvidence>,
    warning: bool,
}
fn component(v: &b::SchemaComponent) -> Component {
    Component {
        kind: match v.kind {
            b::ComponentKind::SimpleType => ComponentKind::SimpleType,
            b::ComponentKind::ComplexType => ComponentKind::ComplexType,
            b::ComponentKind::Attribute => ComponentKind::Attribute,
            b::ComponentKind::Element => ComponentKind::Element,
        },
        name: v.name.as_ref().map(Name::project),
        global: v.global,
        document_uri: v.document_uri.clone(),
        schema_span: v.schema_span,
    }
}
fn component_size(v: &b::SchemaComponent, s: &mut Size) -> Result<(), Failure> {
    s.add(std::mem::size_of::<Component>())?;
    if let Some(n) = &v.name {
        Name::measure(n, s)?;
    }
    if let Some(uri) = &v.document_uri {
        s.add(uri.len())?;
    }
    Ok(())
}
fn error_fields<'a>(
    e: &'a ValidationError,
    c: &Control<'_>,
    limit: usize,
) -> Result<Fields<'a>, Failure> {
    // Admit all available fields first. element_path may lazily allocate a
    // backend rendering, so additionally pre-admit its actual path-name bytes
    // before asking for it. An explicitly stored local path is only borrowed.
    let mut fields = Fields {
        code: e.constraint,
        message: &e.message,
        location: e.location.as_ref(),
        subject: e.subject(),
        path: e.instance_path(),
        element_path: None,
        evidence: e.evidence(),
        warning: false,
    };
    let mut size = Size::new(limit);
    size.add(measure(&fields, c, limit)?)?;
    if let Some(path) = e.instance_path() {
        for step in path.steps() {
            if step.position().is_some() {
                size.add(step.name().local_name().len())?;
                size.add(1)?;
            }
        }
    }
    fields.element_path = e.element_path();
    Ok(fields)
}

fn measure(f: &Fields<'_>, c: &Control<'_>, limit: usize) -> Result<usize, Failure> {
    let mut s = Size::new(limit);
    s.add(std::mem::size_of::<Event>())?;
    s.add(f.code.len())?;
    s.add(f.message.len())?;
    if let Some(l) = f.location {
        s.add(std::mem::size_of::<RawLocation>())?;
        s.add(l.base_uri.len())?;
    }
    if let Some(subject) = f.subject {
        Name::measure(subject.name(), &mut s)?;
    }
    if let Some(path) = f.path {
        // InstancePath::steps itself allocates beyond 32 entries. Bound that
        // temporary before requesting the iterator, as well as owned steps.
        let temporary = mul(path.len(), std::mem::size_of::<&str>())?;
        if temporary > c.limits.temporary_bytes {
            return Err(Failure::Limit {
                dimension: "path_temporary_bytes",
                limit: c.limits.temporary_bytes,
            });
        }
        if path.len()
            > c.limits
                .depth
                .checked_add(1)
                .ok_or(Failure::ArithmeticOverflow)?
        {
            return Err(Failure::SourceJoin);
        }
        s.add(mul(path.len(), std::mem::size_of::<PathStep>())?)?;
        for step in path.steps() {
            Name::measure(step.name(), &mut s)?;
        }
    }
    if let Some(p) = f.element_path {
        s.add(p.len())?;
    }
    if let Some(e) = f.evidence {
        s.add(std::mem::size_of::<Evidence>())?;
        if let Some(cause) = &e.cause {
            NativeCause::measure(cause, &mut s)?;
        }
        for v in [
            &e.governing_type,
            &e.actual_type,
            &e.component,
            &e.facet_origin,
        ]
        .into_iter()
        .flatten()
        {
            component_size(v, &mut s)?;
        }
    }
    s.add(std::mem::size_of::<SourceJoin>())?;
    Ok(s.bytes)
}
fn project(f: Fields<'_>, source: Option<SourceJoin>) -> Result<Diagnostic, Failure> {
    let subject = f
        .subject
        .map(|s| match s {
            b::ValidationSubject::Element(n) => Ok(Subject::Element(Name::project(n))),
            b::ValidationSubject::Attribute(n) => Ok(Subject::Attribute(Name::project(n))),
            _ => Err(Failure::UnsupportedEvidence),
        })
        .transpose()?;
    let path = f
        .path
        .map(|p| {
            p.steps()
                .map(|s| match s {
                    b::PathStep::Element { name, position, .. } => Ok(PathStep::Element {
                        name: Name::project(name),
                        position: *position,
                    }),
                    b::PathStep::Attribute { name, .. } => Ok(PathStep::Attribute {
                        name: Name::project(name),
                    }),
                    _ => Err(Failure::UnsupportedEvidence),
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(Diagnostic {
        code: f.code.to_owned(),
        message: String::new(),
        raw_location: f.location.map(|l| RawLocation {
            base_uri: l.base_uri.clone(),
            line: l.line,
            column: l.column,
        }),
        subject,
        path,
        element_path: f.element_path.map(str::to_owned),
        evidence: f.evidence.map(|e| Evidence {
            cause: e.cause.as_ref().map(NativeCause::project),
            governing_type: e.governing_type.as_ref().map(component),
            actual_type: e.actual_type.as_ref().map(component),
            component: e.component.as_ref().map(component),
            facet_origin: e.facet_origin.as_ref().map(component),
        }),
        evidence_availability: if f.warning {
            EvidenceAvailability::NotExportedByWarningApi
        } else if f.evidence.is_some() {
            EvidenceAvailability::Captured
        } else {
            EvidenceAvailability::Unavailable
        },
        source,
        triggering_occurrence: None,
        causal_parent: None,
    })
}
fn equal(n: &einvoice_source::Name, e: &b::ExpandedName) -> bool {
    n.local() == e.local_name() && n.namespace_uri() == e.namespace_uri()
}
pub(crate) fn preorder(node: einvoice_source::Node<'_>, c: &Control<'_>) -> Result<usize, Failure> {
    for (ordinal, candidate) in c.source.nodes().enumerate() {
        c.work(1)?;
        if candidate.identity() == node.identity() {
            return Ok(ordinal);
        }
    }
    Err(Failure::SourceJoin)
}
fn join(f: &Fields<'_>, c: &Control<'_>) -> Result<Option<SourceJoin>, Failure> {
    let (Some(path), Some(subject)) = (f.path, f.subject) else {
        return Ok(None);
    };
    let mut steps = path.steps();
    // Every run pushes this exact full-document root; no fragment path is joined.
    let mut node = c.source.root_element();
    match steps.next() {
        Some(b::PathStep::Element {
            name, position: 1, ..
        }) if equal(node.name().expect("source element"), name) => {}
        _ => return Err(Failure::SourceJoin),
    }
    let mut attribute = None;
    for step in steps {
        c.work(1)?;
        match step {
            b::PathStep::Element { name, position, .. } if attribute.is_none() => {
                let mut found = None;
                for child in node.children() {
                    c.work(1)?;
                    if child.kind() == einvoice_source::NodeKind::Element
                        && equal(child.name().expect("source element"), name)
                        && child.occurrence() as usize == *position
                    {
                        found = Some(child);
                        break;
                    }
                }
                node = found.ok_or(Failure::SourceJoin)?;
            }
            b::PathStep::Attribute { name, .. } if attribute.is_none() => {
                for (index, a) in node.attributes().enumerate() {
                    c.work(1)?;
                    if equal(a.name(), name) {
                        attribute = Some((index, a));
                        break;
                    }
                }
                if attribute.is_none() {
                    return Err(Failure::SourceJoin);
                }
            }
            _ => return Err(Failure::UnsupportedEvidence),
        }
    }
    let owner = node.covering_span().into();
    let owner_preorder = preorder(node, c)?;
    let owner_reference = SourceReference::Node {
        preorder: owner_preorder,
    };
    let subject_reference = attribute.map_or(owner_reference, |(attribute, _)| {
        SourceReference::Attribute {
            owner_preorder,
            attribute,
        }
    });
    let (span, value, loc) = match (subject, attribute) {
        (b::ValidationSubject::Element(name), None)
            if equal(node.name().expect("source element"), name) =>
        {
            (owner, None, node.location())
        }
        (b::ValidationSubject::Attribute(name), Some((_, a))) if equal(a.name(), name) => (
            a.token().span().into(),
            Some(a.value_span().into()),
            a.token().location(),
        ),
        _ => return Err(Failure::SourceJoin),
    };
    Ok(Some(SourceJoin {
        owner,
        owner_reference,
        subject_reference,
        subject: span,
        attribute_value: value,
        line: loc.line,
        column: loc.column,
    }))
}
pub(crate) struct Sink<'a, 'b>(pub &'a Control<'b>);
impl Sink<'_, '_> {
    fn callback(&self) -> Result<(), Failure> {
        let mut s = self.0.state.borrow_mut();
        s.callbacks = s
            .callbacks
            .checked_add(1)
            .ok_or(Failure::ArithmeticOverflow)?;
        Ok(())
    }
    fn prepare(&self, f: Fields<'_>) -> Result<Diagnostic, Failure> {
        let c = self.0;
        c.poll()?;
        let state = c.state.borrow();
        if state.events.len() >= c.limits.diagnostic_count {
            return Err(Failure::Limit {
                dimension: "diagnostic_count",
                limit: c.limits.diagnostic_count,
            });
        }
        let remaining = c
            .limits
            .diagnostic_bytes
            .checked_sub(state.bytes)
            .ok_or(Failure::ArithmeticOverflow)?;
        drop(state);
        let size = measure(&f, c, remaining)?;
        // Size enumeration, copying and path joins all consume the same checked
        // cumulative work allowance. No debug rendering is used for admission.
        c.work(size)?;
        let source = join(&f, c)?;
        let event = project(f, source)?;
        let mut state = c.state.borrow_mut();
        state
            .events
            .try_reserve(1)
            .map_err(|_| Failure::Allocation)?;
        state.bytes = state
            .bytes
            .checked_add(size)
            .ok_or(Failure::ArithmeticOverflow)?;
        Ok(event)
    }
    fn retain(&self, result: Result<Diagnostic, Failure>, severity: Severity, message: String) {
        match result {
            Ok(mut diagnostic) => {
                diagnostic.message = message;
                let mut state = self.0.state.borrow_mut();
                let ordinal = state.events.len();
                if severity == Severity::Error {
                    let Some(errors) = state.errors.checked_add(1) else {
                        drop(state);
                        let _ = self.0.fail::<()>(Failure::ArithmeticOverflow);
                        return;
                    };
                    state.errors = errors;
                }
                state.events.push(Event {
                    ordinal,
                    severity,
                    diagnostic,
                });
            }
            Err(e) => {
                let e = match e {
                    Failure::Limit {
                        dimension: "diagnostic_bytes",
                        ..
                    } => Failure::Limit {
                        dimension: "diagnostic_bytes",
                        limit: self.0.limits.diagnostic_bytes,
                    },
                    other => other,
                };
                let _ = self.0.fail::<()>(e);
            }
        }
    }
}
impl ValidationSink for Sink<'_, '_> {
    fn stop_requested(&self) -> bool {
        self.0.poll().is_err()
    }
    fn on_error(&mut self, e: ValidationError) {
        if let Err(f) = self.callback() {
            let _ = self.0.fail::<()>(f);
            return;
        }
        let state = self.0.state.borrow();
        let limit = self
            .0
            .limits
            .diagnostic_bytes
            .checked_sub(state.bytes)
            .ok_or(Failure::ArithmeticOverflow);
        let count = state.events.len();
        drop(state);
        let result = if count >= self.0.limits.diagnostic_count {
            Err(Failure::Limit {
                dimension: "diagnostic_count",
                limit: self.0.limits.diagnostic_count,
            })
        } else {
            self.0
                .poll()
                .and(limit)
                .and_then(|limit| error_fields(&e, self.0, limit))
                .and_then(|f| self.prepare(f))
        };
        self.retain(result, Severity::Error, e.message);
    }
    fn on_warning(&mut self, w: ValidationWarning) {
        if let Err(f) = self.callback() {
            let _ = self.0.fail::<()>(f);
            return;
        }
        let result = self.prepare(Fields {
            code: w.code,
            message: &w.message,
            location: w.location.as_ref(),
            subject: w.subject(),
            path: w.instance_path(),
            element_path: None,
            evidence: None,
            warning: true,
        });
        self.retain(result, Severity::Warning, w.message);
    }
}
/// Terminal errors are retained separately even when the callback allowance is
/// exhausted. They have a separate explicit byte allowance and no fake ordinal.
pub(crate) fn terminal(
    e: &ValidationError,
    c: &Control<'_>,
    remaining: &mut usize,
) -> Result<Diagnostic, Failure> {
    let normalize = |f| match f {
        Failure::Limit {
            dimension: "diagnostic_bytes",
            ..
        } => Failure::Limit {
            dimension: "terminal_bytes",
            limit: c.limits.terminal_bytes,
        },
        other => other,
    };
    let f = error_fields(e, c, *remaining).map_err(normalize)?;
    let size = measure(&f, c, *remaining).map_err(normalize)?;
    *remaining = remaining
        .checked_sub(size)
        .ok_or(Failure::ArithmeticOverflow)?;
    let mut d = project(f, None)?;
    d.message = e.message.clone();
    Ok(d)
}
