use crate::budget::Meter;
use crate::decode::Coordinates;
use crate::document::{
    AttrRecord, DeclRecord, DecodedSpan, LedgerKind, Lexeme, NodeKind, Owner, RawName, Record,
};
use crate::error::{limit, Result};
use crate::SourceError;
use std::collections::HashSet;
use xmlparser::{ElementEnd, StrSpan, Token};

pub(crate) struct Lexical {
    pub records: Vec<Record>,
    pub ledger: Vec<Lexeme>,
    pub values: Vec<DecodedSpan>,
}
fn raw_name(prefix: StrSpan<'_>, local: StrSpan<'_>, m: &mut Meter<'_>) -> Result<RawName> {
    let size = prefix
        .len()
        .checked_add(local.len())
        .and_then(|n| n.checked_add(usize::from(!prefix.is_empty())))
        .ok_or_else(|| limit("name_bytes", m.limits.name_bytes))?;
    if size > m.limits.name_bytes {
        return Err(limit("name_bytes", m.limits.name_bytes));
    }
    Ok(RawName {
        prefix: prefix.range().into(),
        local: local.range().into(),
    })
}
impl Lexical {
    fn node(&mut self, n: Record, m: &mut Meter<'_>) -> Result<u32> {
        if self.records.len() >= m.limits.nodes {
            return Err(limit("nodes", m.limits.nodes));
        }
        let id = u32::try_from(self.records.len()).map_err(|_| limit("node_id", m.limits.nodes))?;
        if let Some(parent) = n.parent {
            m.push(&mut self.records[parent as usize].children, id)?;
        }
        m.push(&mut self.records, n)?;
        Ok(id)
    }
    fn token(
        &mut self,
        kind: LedgerKind,
        span: DecodedSpan,
        value: Option<DecodedSpan>,
        owner: Owner,
        m: &mut Meter<'_>,
    ) -> Result<u32> {
        if self.ledger.len() >= m.limits.lexical_tokens {
            return Err(limit("lexical_tokens", m.limits.lexical_tokens));
        }
        let id = u32::try_from(self.ledger.len())
            .map_err(|_| limit("token_id", m.limits.lexical_tokens))?;
        if self.ledger.last().is_some_and(|l| l.span.end > span.start) {
            return Err(SourceError::Alignment("overlapping ledger tokens"));
        }
        m.push(
            &mut self.ledger,
            Lexeme {
                kind,
                span,
                value,
                owner,
            },
        )?;
        if let Owner::Node(node) = owner {
            m.piece()?;
            m.push(&mut self.records[node as usize].pieces, id)?;
        }
        Ok(id)
    }
    fn text(
        &mut self,
        parent: u32,
        span: DecodedSpan,
        value: DecodedSpan,
        kind: LedgerKind,
        m: &mut Meter<'_>,
    ) -> Result<()> {
        let existing = self.records[parent as usize]
            .children
            .last()
            .copied()
            .filter(|id| self.records[*id as usize].kind == NodeKind::Text);
        let nonempty = value.start != value.end;
        m.expanded(value.end - value.start)?;
        let owner = if parent != 0 && (nonempty || existing.is_some()) {
            let id = if let Some(id) = existing {
                self.records[id as usize].covering.end = span.end;
                id
            } else {
                self.node(Record::new(NodeKind::Text, Some(parent), span), m)?
            };
            let record = &mut self.records[id as usize];
            let size = value.end - value.start;
            record.admitted_value = record
                .admitted_value
                .checked_add(size)
                .filter(|n| *n <= m.limits.value_bytes)
                .ok_or_else(|| limit("value_bytes", m.limits.value_bytes))?;
            id
        } else {
            parent
        };
        self.token(kind, span, Some(value), Owner::Node(owner), m)?;
        Ok(())
    }
}

pub(crate) fn scan(
    text: &str,
    bom: usize,
    coordinates: &Coordinates,
    m: &mut Meter<'_>,
) -> Result<Lexical> {
    let mut out = Lexical {
        records: Vec::new(),
        ledger: Vec::new(),
        values: Vec::new(),
    };
    out.node(
        Record::new(
            NodeKind::Document,
            None,
            DecodedSpan {
                start: 0,
                end: text.len(),
            },
        ),
        m,
    )?;
    if bom > 0 {
        out.token(
            LedgerKind::Bom,
            DecodedSpan::default(),
            None,
            Owner::Node(0),
            m,
        )?;
    }
    let mut start = 0;
    if text.starts_with("<?xml")
        && text
            .as_bytes()
            .get(5)
            .is_some_and(|c| b" \t\r\n".contains(c))
    {
        let end = text
            .find("?>")
            .ok_or(SourceError::Alignment("header declaration terminator"))?
            + 2;
        if end > m.limits.token_bytes {
            return Err(limit("token_bytes", m.limits.token_bytes));
        }
        // Header grammar is xml-rs's responsibility. xmlparser additionally
        // checks lexical declaration values (references are forbidden there).
        let mut declaration = m.copy(&text[..end])?;
        declaration.replace_range(5..6, " ");
        let token = xmlparser::Tokenizer::from(declaration.as_str()).next();
        match token {
            Some(Ok(Token::Declaration { .. })) => {}
            Some(Err(e)) => {
                return Err(SourceError::Syntax {
                    message: e.to_string(),
                    location: coordinates.lexical_location(text, e.pos()),
                })
            }
            _ => return Err(SourceError::Alignment("lexical XML declaration")),
        }
        out.token(
            LedgerKind::Declaration,
            (0..end).into(),
            None,
            Owner::Node(0),
            m,
        )?;
        start = end;
    }
    let mut stack = Vec::new();
    m.push(&mut stack, 0u32)?;
    let mut current = 0;
    let mut attributes = HashSet::new();
    let mut attribute_count = 0usize;
    for token in xmlparser::Tokenizer::from_fragment(text, start..text.len()) {
        m.work(1)?;
        let token = token.map_err(|e| SourceError::Syntax {
            message: e.to_string(),
            location: coordinates.lexical_location(text, e.pos()),
        })?;
        let span = token.span();
        m.work(span.len())?;
        if span.len() > m.limits.token_bytes {
            return Err(limit("token_bytes", m.limits.token_bytes));
        }
        let parent = *stack
            .last()
            .ok_or(SourceError::Alignment("lexical stack"))?;
        match token {
            Token::ElementStart {
                prefix,
                local,
                span,
            } => {
                if stack.len() > m.limits.depth {
                    return Err(limit("depth", m.limits.depth));
                }
                let mut n = Record::new(NodeKind::Element, Some(parent), span.range().into());
                n.raw_name = raw_name(prefix, local, m)?;
                current = out.node(n, m)?;
                attributes.clear();
                attribute_count = 0;
                out.token(
                    LedgerKind::ElementStart,
                    span.range().into(),
                    None,
                    Owner::Node(current),
                    m,
                )?;
            }
            Token::Attribute {
                prefix,
                local,
                value,
                span,
            } => {
                attribute_count = attribute_count
                    .checked_add(1)
                    .ok_or_else(|| limit("attributes", m.limits.attributes))?;
                if attribute_count > m.limits.attributes {
                    return Err(limit("attributes", m.limits.attributes));
                }
                let name = raw_name(prefix, local, m)?;
                m.work(prefix.len() + local.len())?;
                m.storage(64)?;
                attributes
                    .try_reserve(1)
                    .map_err(|_| limit("allocation", m.limits.storage_bytes))?;
                if !attributes.insert((prefix.as_str(), local.as_str())) {
                    return Err(SourceError::Syntax {
                        message: "duplicate lexical attribute or namespace declaration".into(),
                        location: Some(coordinates.location(text, span.start())),
                    });
                }
                m.expanded(value.len())?;
                m.push(&mut out.values, value.range().into())?;
                let node = &out.records[current as usize];
                let is_ns =
                    prefix.as_str() == "xmlns" || (prefix.is_empty() && local.as_str() == "xmlns");
                if is_ns {
                    // Declaration names are not ordinary resolved attributes:
                    // reserve the raw QName buffer, its parsed fields and the
                    // namespace-stack key clone. Snapshot keys are charged by
                    // admit_namespaces separately; no URI belongs to this name.
                    let qname = prefix
                        .len()
                        .checked_add(local.len())
                        .and_then(|n| n.checked_add(usize::from(!prefix.is_empty())))
                        .ok_or_else(|| limit("string_bytes", m.limits.string_bytes))?;
                    let key = if prefix.is_empty() { 0 } else { local.len() };
                    let payload = qname
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(key))
                        .ok_or_else(|| limit("string_bytes", m.limits.string_bytes))?;
                    m.work(payload)?;
                    m.strings(payload)?;
                    if node.declarations.len() >= m.limits.local_namespaces {
                        return Err(limit("local_namespaces", m.limits.local_namespaces));
                    }
                    if value.len() > m.limits.uri_bytes {
                        return Err(limit("uri_bytes", m.limits.uri_bytes));
                    }
                    let index = node.declarations.len() as u32;
                    let token = out.token(
                        LedgerKind::NamespaceDeclaration,
                        span.range().into(),
                        Some(value.range().into()),
                        Owner::Declaration(current, index),
                        m,
                    )?;
                    m.push(
                        &mut out.records[current as usize].declarations,
                        DeclRecord {
                            prefix: if prefix.is_empty() {
                                DecodedSpan {
                                    start: local.start(),
                                    end: local.start(),
                                }
                            } else {
                                local.range().into()
                            },
                            uri: String::new(),
                            token,
                            value_span: value.range().into(),
                        },
                    )?;
                } else {
                    let index = node.attributes.len() as u32;
                    let token = out.token(
                        LedgerKind::Attribute,
                        span.range().into(),
                        Some(value.range().into()),
                        Owner::Attribute(current, index),
                        m,
                    )?;
                    m.push(
                        &mut out.records[current as usize].attributes,
                        AttrRecord {
                            name: None,
                            raw_name: name,
                            value: String::new(),
                            token,
                            value_span: value.range().into(),
                        },
                    )?;
                }
            }
            Token::ElementEnd { end, span } => match end {
                ElementEnd::Open | ElementEnd::Empty => {
                    admit_namespaces(&out.records, current, text, m)?;
                    out.token(
                        LedgerKind::StartTagEnd,
                        span.range().into(),
                        None,
                        Owner::Node(current),
                        m,
                    )?;
                    if end == ElementEnd::Open {
                        m.push(&mut stack, current)?;
                    } else {
                        out.records[current as usize].covering.end = span.end();
                        out.records[current as usize].subtree_end = out.records.len() as u32;
                    }
                }
                ElementEnd::Close(prefix, local) => {
                    if stack.len() <= 1 {
                        return Err(SourceError::Syntax {
                            message: "unmatched closing tag".into(),
                            location: Some(coordinates.location(text, span.start())),
                        });
                    }
                    let id = stack.pop().ok_or(SourceError::Alignment("closing stack"))?;
                    let closing = raw_name(prefix, local, m)?;
                    admit_name_copies(&out.records, id, closing, true, 3, text, m)?;
                    out.records[id as usize].covering.end = span.end();
                    out.records[id as usize].subtree_end = out.records.len() as u32;
                    out.token(
                        LedgerKind::ElementEnd,
                        span.range().into(),
                        None,
                        Owner::Node(id),
                        m,
                    )?;
                }
            },
            Token::Text { text: t } => {
                let kind = if parent == 0 && t.as_str().bytes().all(|b| b" \t\r\n".contains(&b)) {
                    LedgerKind::DocumentWhitespace
                } else {
                    LedgerKind::Text
                };
                out.text(parent, t.range().into(), t.range().into(), kind, m)?;
            }
            Token::Cdata { text: t, span } => {
                out.text(
                    parent,
                    span.range().into(),
                    t.range().into(),
                    LedgerKind::Cdata,
                    m,
                )?;
            }
            Token::Comment { text: t, span } => {
                m.expanded(t.len())?;
                let id = out.node(
                    Record::new(NodeKind::Comment, Some(parent), span.range().into()),
                    m,
                )?;
                out.token(
                    LedgerKind::Comment,
                    span.range().into(),
                    Some(t.range().into()),
                    Owner::Node(id),
                    m,
                )?;
            }
            Token::ProcessingInstruction {
                target,
                content,
                span,
            } => {
                let mut n = Record::new(
                    NodeKind::ProcessingInstruction,
                    Some(parent),
                    span.range().into(),
                );
                if target.as_str().contains(':') {
                    return Err(SourceError::Syntax {
                        message: "processing-instruction target must be an NCName".into(),
                        location: Some(coordinates.location(text, target.start())),
                    });
                }
                n.raw_name = raw_name(StrSpan::from(""), target, m)?;
                // Include separator whitespace: the parser buffers it even
                // though the XPath PI value excludes it.
                m.expanded(span.end() - target.end() - 2)?;
                let target_payload = target
                    .len()
                    .checked_mul(2)
                    .ok_or_else(|| limit("string_bytes", m.limits.string_bytes))?;
                m.work(target_payload)?;
                m.strings(target_payload)?;
                let id = out.node(n, m)?;
                out.token(
                    LedgerKind::ProcessingInstruction,
                    span.range().into(),
                    content.map(|c| c.range().into()),
                    Owner::Node(id),
                    m,
                )?;
            }
            Token::DtdStart { .. }
            | Token::EmptyDtd { .. }
            | Token::EntityDeclaration { .. }
            | Token::DtdEnd { .. } => return Err(SourceError::UnsupportedDtd),
            Token::Declaration { .. } => {
                return Err(SourceError::Syntax {
                    message: "misplaced XML declaration".into(),
                    location: Some(coordinates.location(text, span.start())),
                })
            }
        }
    }
    // A truncated lexical stream cannot escape to success. xml-rs additionally
    // verifies names, root count, legal document-level content and exact EOF.
    if stack.len() != 1 {
        return Err(SourceError::Syntax {
            message: "unclosed element".into(),
            location: Some(coordinates.location(text, text.len())),
        });
    }
    out.records[0].subtree_end = out.records.len() as u32;
    for i in 1..out.records.len() {
        if out.records[i].subtree_end == 0 {
            out.records[i].subtree_end = i as u32 + 1;
        }
    }
    Ok(out)
}
fn admit_namespaces(records: &[Record], id: u32, text: &str, m: &mut Meter<'_>) -> Result<()> {
    let mut seen = HashSet::new();
    let mut frame = Some(id);
    // xml-rs carries implicit xml, xmlns and the empty default binding. Charge
    // these even though xmlns/empty bindings are absent from the namespace axis.
    let mut effective = 1usize;
    let mut frames = 0usize;
    m.namespace(256)?;
    m.strings(crate::decode::IMPLICIT_NAMESPACE_BYTES)?;
    seen.try_reserve(1)
        .map_err(|_| limit("allocation", m.limits.storage_bytes))?;
    seen.insert("xml");
    while let Some(id) = frame {
        let n = &records[id as usize];
        frames += 1;
        m.work(1)?;
        for d in &n.declarations {
            let prefix = &text[d.prefix.range()];
            let payload = prefix.len() + d.value_span.end - d.value_span.start;
            m.work(payload + 1)?;
            // squash() clones even shadowed bindings before overwriting them.
            // Charge every frame entry, plus a temporary admission-map record.
            m.namespace(payload + 128)?;
            m.strings(payload)?;
            seen.try_reserve(1)
                .map_err(|_| limit("allocation", m.limits.storage_bytes))?;
            if seen.insert(prefix) {
                effective = effective
                    .checked_add(1)
                    .ok_or_else(|| limit("effective_namespaces", m.limits.effective_namespaces))?;
                if effective > m.limits.effective_namespaces {
                    return Err(limit("effective_namespaces", m.limits.effective_namespaces));
                }
            }
        }
        frame = n.parent;
    }
    if effective > m.limits.effective_namespaces {
        return Err(limit("effective_namespaces", m.limits.effective_namespaces));
    }
    let lookups = records[id as usize].attributes.len() + 1;
    let lookup_work = frames
        .checked_mul(lookups)
        .ok_or_else(|| limit("work", m.limits.work))?;
    m.work(lookup_work)?;
    // The parser resolves namespaces for the entire event before yielding it.
    // Charge QName buffers, parsed fields, its element stack/queued end-event
    // clone and all attribute URI fanout before the semantic pass starts.
    let node = &records[id as usize];
    admit_name_copies(records, id, node.raw_name, true, 4, text, m)?;
    for attribute in &node.attributes {
        admit_name_copies(records, id, attribute.raw_name, false, 3, text, m)?;
    }
    Ok(())
}

fn admit_name_copies(
    records: &[Record],
    owner: u32,
    name: RawName,
    element: bool,
    copies: usize,
    text: &str,
    m: &mut Meter<'_>,
) -> Result<()> {
    let prefix = &text[name.prefix.range()];
    let mut uri_bytes = 0usize;
    if prefix == "xml" {
        uri_bytes = "http://www.w3.org/XML/1998/namespace".len();
    } else if prefix == "xmlns" {
        uri_bytes = "http://www.w3.org/2000/xmlns/".len();
    } else if element || !prefix.is_empty() {
        let mut frame = Some(owner);
        'resolve: while let Some(id) = frame {
            m.work(1)?;
            let node = &records[id as usize];
            for declaration in &node.declarations {
                let candidate = &text[declaration.prefix.range()];
                m.work(prefix.len().max(candidate.len()) + 1)?;
                if prefix == candidate {
                    // DTDs are forbidden; XML predefined/character references
                    // and literal normalization cannot expand this UTF-8 span.
                    uri_bytes = declaration.value_span.end - declaration.value_span.start;
                    break 'resolve;
                }
            }
            frame = node.parent;
        }
    }
    let payload = prefix
        .len()
        .checked_add(name.local.end - name.local.start)
        .and_then(|n| n.checked_add(usize::from(!prefix.is_empty())))
        .and_then(|n| n.checked_add(uri_bytes))
        .and_then(|n| n.checked_mul(copies))
        .ok_or_else(|| limit("string_bytes", m.limits.string_bytes))?;
    m.work(payload)?;
    m.strings(payload)
}

#[cfg(test)]
mod declaration_admission_tests {
    #[test]
    fn declaration_fields_and_stack_key_are_admitted_before_semantic_parsing() {
        for length in [257, 2049, 3901] {
            let text = format!("<r xmlns:{}='u'/>", "p".repeat(length));
            let generous = crate::SourceLimits::default();
            let coordinates = crate::decode::Coordinates::build(
                &text,
                &mut crate::budget::Meter::new(&generous, None),
            )
            .unwrap();
            let limits = crate::SourceLimits {
                string_bytes: length * 2,
                ..generous
            };
            assert!(matches!(
                super::scan(
                    &text,
                    0,
                    &coordinates,
                    &mut crate::budget::Meter::new(&limits, None)
                ),
                Err(crate::SourceError::Resource(crate::Resource {
                    dimension: "string_bytes",
                    ..
                }))
            ));
            super::scan(
                &text,
                0,
                &coordinates,
                &mut crate::budget::Meter::new(&generous, None),
            )
            .unwrap();
        }
    }
}
