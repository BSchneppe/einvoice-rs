use crate::document::{Attribute, Node, NodeKind};
use crate::error::Result;
use crate::{QueryBudget, SourceError};
use std::fmt::Write;

fn emit(writer: &mut impl Write, text: &str, budget: &mut QueryBudget) -> Result<()> {
    budget.output(text.len())?;
    writer.write_str(text).map_err(|_| SourceError::Output)
}
pub(crate) fn value(
    node: Node<'_>,
    writer: &mut impl Write,
    budget: &mut QueryBudget,
) -> Result<()> {
    budget.work(1)?;
    if matches!(node.kind(), NodeKind::Document | NodeKind::Element) {
        // Preorder subtree interval: no recursion or eager ancestor string copies.
        for record in &node.doc.records[node.id as usize + 1..node.record().subtree_end as usize] {
            budget.work(1)?;
            if record.kind == NodeKind::Text {
                emit(writer, &record.value, budget)?;
            }
        }
    } else {
        emit(writer, node.content().unwrap_or(""), budget)?;
    }
    Ok(())
}
struct Collector(String);
impl Write for Collector {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.try_reserve(s.len()).map_err(|_| std::fmt::Error)?;
        self.0.push_str(s);
        Ok(())
    }
}
fn collection_error(error: SourceError, budget: &QueryBudget) -> SourceError {
    if error == SourceError::Output {
        budget.allocation_error()
    } else {
        error
    }
}
pub(crate) fn collect_value(node: Node<'_>, budget: &mut QueryBudget) -> Result<String> {
    let mut out = Collector(String::new());
    value(node, &mut out, budget).map_err(|e| collection_error(e, budget))?;
    Ok(out.0)
}
fn number(writer: &mut impl Write, mut n: u32, budget: &mut QueryBudget) -> Result<()> {
    let mut buf = [0u8; 10];
    let mut at = buf.len();
    loop {
        at -= 1;
        buf[at] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    emit(
        writer,
        std::str::from_utf8(&buf[at..])
            .map_err(|_| SourceError::Alignment("decimal path index"))?,
        budget,
    )
}
pub(crate) fn path(
    node: Node<'_>,
    attribute: Option<Attribute<'_>>,
    budget: &mut QueryBudget,
) -> Result<String> {
    let mut out = Collector(String::new());
    write_path(node, attribute, &mut out, budget).map_err(|e| collection_error(e, budget))?;
    Ok(out.0)
}
fn write_path(
    node: Node<'_>,
    attribute: Option<Attribute<'_>>,
    out: &mut impl Write,
    budget: &mut QueryBudget,
) -> Result<()> {
    let mut ancestors = Vec::new();
    let mut current = Some(node);
    while let Some(n) = current {
        budget.work(1)?;
        ancestors
            .try_reserve(1)
            .map_err(|_| budget.allocation_error())?;
        ancestors.push(n);
        current = n.parent();
    }
    if node.kind() == NodeKind::Document {
        emit(out, "/", budget)?;
    }
    for n in ancestors
        .into_iter()
        .rev()
        .filter(|n| n.kind() != NodeKind::Document)
    {
        emit(out, "/", budget)?;
        match n.kind() {
            NodeKind::Element => {
                let name = n.name().ok_or(SourceError::Alignment("path name"))?;
                emit(out, "Q{", budget)?;
                emit(out, name.namespace_uri(), budget)?;
                emit(out, "}", budget)?;
                emit(out, name.local(), budget)?;
            }
            NodeKind::Text => emit(out, "text()", budget)?,
            NodeKind::Comment => emit(out, "comment()", budget)?,
            NodeKind::ProcessingInstruction => {
                emit(out, "processing-instruction(\"", budget)?;
                emit(
                    out,
                    n.name()
                        .ok_or(SourceError::Alignment("PI path name"))?
                        .local(),
                    budget,
                )?;
                emit(out, "\")", budget)?;
            }
            NodeKind::Document => unreachable!(),
        }
        emit(out, "[", budget)?;
        number(out, n.occurrence(), budget)?;
        emit(out, "]", budget)?;
    }
    if let Some(a) = attribute {
        emit(out, "/@Q{", budget)?;
        emit(out, a.name().namespace_uri(), budget)?;
        emit(out, "}", budget)?;
        emit(out, a.name().local(), budget)?;
    }
    Ok(())
}
