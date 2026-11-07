use crate::budget::Meter;
use crate::decode::{self, Coordinates, Normalized};
use crate::document::{Name, NodeKind, RawName, Record, SourceDocument};
use crate::error::{limit, Result};
use crate::lexical;
use crate::{SourceError, SourceLimits};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use xml::common::Position;
use xml::name::OwnedName;
use xml::reader::XmlEvent;

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn parse(
    raw: &[u8],
    limits: &SourceLimits,
    cancel: Option<&AtomicBool>,
) -> Result<Arc<SourceDocument>> {
    limits.validate()?;
    if raw.len() > limits.original_bytes {
        return Err(limit("original_bytes", limits.original_bytes));
    }
    let mut meter = Meter::new(limits, cancel);
    meter.work(raw.len())?;
    decode::reject_dtd(raw)?;
    let (encoding, bom) = decode::header(raw, &mut meter)?;
    let decoded = decode::decode(raw, encoding, bom, &mut meter)?;
    let coordinates = Coordinates::build(&decoded, &mut meter)?;
    let mut lex = lexical::scan(&decoded, bom, &coordinates, &mut meter)?;
    let normalized = Normalized::build(&decoded, &lex.values, &mut meter)?;
    let parser_input = normalized.encode(encoding, bom, &mut meter)?;
    // Charge scanning bytes before the only full semantic parse. Event and
    // name/frame work is charged separately as it is performed.
    meter.work(parser_input.len())?;
    decode::admit_parser(&mut meter)?;
    let mut reader = decode::config(&meter).create_reader(parser_input.as_slice());
    let mut stack = Vec::new();
    meter.push(&mut stack, 0u32)?;
    let mut next = 1usize;
    let mut text = None;
    let mut roots = 0usize;
    loop {
        meter.work(1)?;
        let event = reader.next().map_err(|e| {
            let location = normalized.original_location(&decoded, &coordinates, e.position());
            match location {
                Some(location) => decode::parser_error(e, Some(location)),
                None => SourceError::Alignment("parser error coordinate outside normalized view"),
            }
        })?;
        let parent = *stack.last().ok_or(SourceError::Alignment("SAX stack"))?;
        match event {
            XmlEvent::StartDocument { .. } => {}
            XmlEvent::StartElement {
                name,
                attributes,
                namespace,
            } => {
                text = None;
                let id = claim(&lex.records, &mut next, NodeKind::Element, parent)?;
                if parent == 0 {
                    roots += 1;
                }
                let n = &mut lex.records[id as usize];
                n.name = Some(adopt_name(name, n.raw_name, &decoded, &mut meter)?);
                if attributes.len() != n.attributes.len() {
                    return Err(SourceError::Alignment("attribute count"));
                }
                // The lexical set closes xmlns map collapse; expanded ordinary
                // attributes additionally need uniqueness across prefix aliases.
                let mut expanded = HashSet::new();
                for a in &attributes {
                    meter.work(
                        a.name.local_name.len()
                            + a.name.namespace.as_deref().map_or(0, str::len)
                            + 1,
                    )?;
                    meter.storage(64)?;
                    expanded
                        .try_reserve(1)
                        .map_err(|_| limit("allocation", limits.storage_bytes))?;
                    if !expanded.insert((
                        a.name.namespace.as_deref().unwrap_or(""),
                        a.name.local_name.as_str(),
                    )) {
                        return Err(SourceError::Syntax {
                            message: "duplicate expanded attribute name".into(),
                            location: Some(coordinates.location(&decoded, n.covering.start)),
                        });
                    }
                }
                drop(expanded);
                for (a, event) in n.attributes.iter_mut().zip(attributes) {
                    a.name = Some(adopt_name(event.name, a.raw_name, &decoded, &mut meter)?);
                    admit_value(&event.value, &mut meter)?;
                    a.value = event.value;
                }
                for d in &mut n.declarations {
                    let prefix = &decoded[d.prefix.range()];
                    meter.work(prefix.len() + 1)?;
                    let uri = namespace
                        .get(prefix)
                        .ok_or(SourceError::Alignment("namespace declaration missing"))?;
                    if uri.len() > limits.uri_bytes {
                        return Err(limit("uri_bytes", limits.uri_bytes));
                    }
                    if (prefix != "xml" && uri == "http://www.w3.org/XML/1998/namespace")
                        || uri == "http://www.w3.org/2000/xmlns/"
                    {
                        return Err(SourceError::Syntax {
                            message: "reserved namespace URI binding".into(),
                            location: Some(coordinates.location(&decoded, d.value_span.start)),
                        });
                    }
                    d.uri = meter.copy(uri)?;
                }
                meter.push(&mut stack, id)?;
            }
            XmlEvent::EndElement { name } => {
                text = None;
                if stack.len() <= 1 {
                    return Err(SourceError::Alignment("SAX closing stack"));
                }
                let id = stack
                    .pop()
                    .ok_or(SourceError::Alignment("SAX closing stack"))?;
                let n = &lex.records[id as usize];
                if next != n.subtree_end as usize
                    || !same_name(
                        n.name
                            .as_ref()
                            .ok_or(SourceError::Alignment("closing name"))?,
                        &name,
                    )
                {
                    return Err(SourceError::Alignment("SAX subtree/name"));
                }
            }
            XmlEvent::Characters(value) | XmlEvent::Whitespace(value) | XmlEvent::CData(value) => {
                if parent != 0 && !value.is_empty() {
                    let id = if let Some(id) = text {
                        id
                    } else {
                        let id = claim(&lex.records, &mut next, NodeKind::Text, parent)?;
                        text = Some(id);
                        id
                    };
                    meter.work(value.len())?;
                    meter.append(&mut lex.records[id as usize].value, &value)?;
                }
            }
            XmlEvent::Comment(value) => {
                text = None;
                let id = claim(&lex.records, &mut next, NodeKind::Comment, parent)?;
                admit_value(&value, &mut meter)?;
                lex.records[id as usize].value = value;
            }
            XmlEvent::ProcessingInstruction { name, data } => {
                text = None;
                let id = claim(
                    &lex.records,
                    &mut next,
                    NodeKind::ProcessingInstruction,
                    parent,
                )?;
                let raw = lex.records[id as usize].raw_name;
                if name != decoded[raw.local.range()] {
                    return Err(SourceError::Alignment("PI target"));
                }
                // PI target copies were admitted lexically.
                lex.records[id as usize].name = Some(Name {
                    prefix: None,
                    local: name,
                    uri: String::new(),
                });
                let mut value = data.unwrap_or_default();
                // xml-rs consumes one separator, whereas the XPath PI value
                // excludes the entire leading XML-whitespace separator.
                let separator =
                    value.len() - value.trim_start_matches([' ', '\t', '\r', '\n']).len();
                value.drain(..separator);
                admit_value(&value, &mut meter)?;
                lex.records[id as usize].value = value;
            }
            XmlEvent::EndDocument => {
                if stack.len() != 1 || next != lex.records.len() || roots != 1 {
                    return Err(SourceError::Alignment("semantic EOF/root/ledger"));
                }
                break;
            }
        }
    }
    drop(reader);
    cache_occurrences(&mut lex.records, &mut meter)?;
    meter.check()?;
    // Original ownership is acquired only after successful bounded construction.
    // Arc's allocator path is not fallible; this logical payload was checked.
    meter.storage(raw.len())?;
    let mut original = Vec::new();
    original
        .try_reserve_exact(raw.len())
        .map_err(|_| limit("allocation", limits.storage_bytes))?;
    original.extend_from_slice(raw);
    let identity = NEXT_DOCUMENT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| SourceError::Alignment("document identity exhausted"))?;
    Ok(Arc::new(SourceDocument {
        identity,
        raw: Arc::from(original),
        decoded,
        encoding,
        bom,
        coordinates,
        records: lex.records,
        ledger: lex.ledger,
    }))
}
fn claim(records: &[Record], next: &mut usize, kind: NodeKind, parent: u32) -> Result<u32> {
    let n = records
        .get(*next)
        .ok_or(SourceError::Alignment("extra semantic node"))?;
    if n.kind != kind || n.parent != Some(parent) {
        return Err(SourceError::Alignment("lexical/semantic kind or parent"));
    }
    let id = u32::try_from(*next).map_err(|_| SourceError::Alignment("semantic node index"))?;
    *next += 1;
    Ok(id)
}
fn same_name(a: &Name, b: &OwnedName) -> bool {
    a.prefix.as_deref() == b.prefix.as_deref()
        && a.local == b.local_name
        && a.uri == b.namespace.as_deref().unwrap_or("")
}
fn adopt_name(name: OwnedName, raw: RawName, decoded: &str, m: &mut Meter<'_>) -> Result<Name> {
    let prefix = name.prefix.as_deref().unwrap_or("");
    let uri = name.namespace.as_deref().unwrap_or("");
    m.work(prefix.len() + name.local_name.len() + uri.len() + 1)?;
    if prefix != &decoded[raw.prefix.range()] || name.local_name != decoded[raw.local.range()] {
        return Err(SourceError::Alignment("lexical QName"));
    }
    if uri.len() > m.limits.uri_bytes {
        return Err(limit("uri_bytes", m.limits.uri_bytes));
    }
    // All parser name/URI copies, including this moved value, were admitted
    // during the lexical pass. Do not charge that reservation again here.
    Ok(Name {
        prefix: name.prefix,
        local: name.local_name,
        uri: name.namespace.unwrap_or_default(),
    })
}
fn admit_value(value: &str, m: &mut Meter<'_>) -> Result<()> {
    if value.len() > m.limits.value_bytes {
        return Err(limit("value_bytes", m.limits.value_bytes));
    }
    // Parser value storage was reserved before the semantic pass.
    m.work(value.len())
}
#[derive(Hash, PartialEq, Eq)]
enum Occurrence<'a> {
    Element(&'a str, &'a str),
    Text,
    Comment,
    Pi(&'a str),
}
fn cache_occurrences(records: &mut [Record], m: &mut Meter<'_>) -> Result<()> {
    let mut ordinals = Vec::new();
    for _ in 0..records.len() {
        m.push(&mut ordinals, 1u32)?;
    }
    for parent in records.iter() {
        let mut counts = HashMap::new();
        for id in &parent.children {
            let n = &records[*id as usize];
            let key = match n.kind {
                NodeKind::Element => {
                    let name = n
                        .name
                        .as_ref()
                        .ok_or(SourceError::Alignment("occurrence name"))?;
                    m.work(name.uri.len() + name.local.len())?;
                    Occurrence::Element(&name.uri, &name.local)
                }
                NodeKind::ProcessingInstruction => {
                    let name = n
                        .name
                        .as_ref()
                        .ok_or(SourceError::Alignment("occurrence PI"))?;
                    m.work(name.local.len())?;
                    Occurrence::Pi(&name.local)
                }
                NodeKind::Text => Occurrence::Text,
                NodeKind::Comment => Occurrence::Comment,
                NodeKind::Document => return Err(SourceError::Alignment("child document")),
            };
            m.work(1)?;
            m.storage(64)?;
            counts
                .try_reserve(1)
                .map_err(|_| limit("allocation", m.limits.storage_bytes))?;
            let count = counts.entry(key).or_insert(0u32);
            *count = count
                .checked_add(1)
                .ok_or_else(|| limit("occurrence", m.limits.nodes))?;
            ordinals[*id as usize] = *count;
        }
    }
    for (n, ordinal) in records.iter_mut().zip(ordinals) {
        n.occurrence = ordinal;
    }
    Ok(())
}
