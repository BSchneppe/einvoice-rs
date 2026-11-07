use super::meter::Meter;
use crate::{Error, Failure, Result};
use einvoice_source::{
    Attribute, DocumentOrder, Namespace, Node, NodeIdentity, NodeKind, SourceDocument,
};
use validation_ir::NodeTest;

/// Original immutable source occurrence, including separate attributes/namespaces.
#[derive(Clone, Copy, Debug)]
pub enum SourceNode<'d> {
    /// Ordinary document/element/text/comment/PI.
    Node(Node<'d>),
    /// Ordinary attribute.
    Attribute(Attribute<'d>),
    /// Effective namespace node, qualified by its exposing element.
    Namespace(Namespace<'d>),
}
impl<'d> SourceNode<'d> {
    /// Document-qualified source identity.
    pub fn identity(self) -> NodeIdentity {
        match self {
            Self::Node(n) => n.identity(),
            Self::Attribute(a) => a.identity(),
            Self::Namespace(n) => n.identity(),
        }
    }
    /// Original source order; foreign documents are incomparable.
    pub fn order(self) -> DocumentOrder {
        match self {
            Self::Node(n) => n.order(),
            Self::Attribute(a) => a.order(),
            Self::Namespace(n) => n.order(),
        }
    }
    /// Exact lexical QName evidence, without manufacturing a prefix.
    pub fn name(self) -> Option<&'d einvoice_source::Name> {
        match self {
            Self::Node(n) => n.name(),
            Self::Attribute(a) => Some(a.name()),
            Self::Namespace(_) => None,
        }
    }
    pub(super) fn parent(self) -> Option<Self> {
        match self {
            Self::Node(n) => n.parent().map(Self::Node),
            Self::Attribute(a) => Some(Self::Node(a.owner())),
            Self::Namespace(n) => Some(Self::Node(n.owner())),
        }
    }
    pub(super) fn belongs(self, doc: &SourceDocument) -> bool {
        self.order().partial_cmp(&doc.document().order()).is_some()
    }
}
fn matches(node: SourceNode<'_>, test: &NodeTest, axis: &str) -> bool {
    if let NodeTest::Kind { name } = test {
        return match name.as_str() {
            "node" => true,
            "text" => matches!(node, SourceNode::Node(n) if n.kind() == NodeKind::Text),
            "comment" => matches!(node, SourceNode::Node(n) if n.kind() == NodeKind::Comment),
            "processing-instruction" => {
                matches!(node, SourceNode::Node(n) if n.kind() == NodeKind::ProcessingInstruction)
            }
            _ => false, // preparation rejects other kind tests
        };
    }
    let principal = match axis {
        "attribute" => matches!(node, SourceNode::Attribute(_)),
        "namespace" => matches!(node, SourceNode::Namespace(_)),
        _ => matches!(node, SourceNode::Node(n) if n.kind() == NodeKind::Element),
    };
    if !principal {
        return false;
    }
    let (uri, local) = match node {
        SourceNode::Namespace(n) => ("", n.prefix()),
        _ => node
            .name()
            .map_or(("", ""), |n| (n.namespace_uri(), n.local())),
    };
    match test {
        NodeTest::Any => true,
        NodeTest::Name { name } => name.namespace == uri && name.local == local,
        NodeTest::Namespace { namespace, .. } => namespace == uri,
        NodeTest::Local { local: wanted } => wanted == local,
        NodeTest::Kind { .. } => false,
    }
}
pub(super) fn axis<'d>(
    doc: &'d SourceDocument,
    context: SourceNode<'d>,
    axis: &str,
    test: &NodeTest,
    proximity: bool,
    meter: &mut Meter,
) -> Result<Vec<SourceNode<'d>>> {
    if !context.belongs(doc) {
        return Err(Error::Fatal(Failure::ForeignScope));
    }
    let mut out = Vec::new();
    let mut admit = |n: SourceNode<'d>, selected: bool| -> Result<()> {
        meter.work(1)?;
        // Charge name tests before the string comparisons.
        if let Some(name) = n.name() {
            meter.work(
                name.local()
                    .len()
                    .checked_add(name.namespace_uri().len())
                    .ok_or_else(crate::error::size_overflow)?,
            )?;
        }
        if selected && matches(n, test, axis) {
            meter.push(&mut out, n)?;
        }
        Ok(())
    };
    match axis {
        "self" => admit(context, true)?,
        "parent" => {
            if let Some(n) = context.parent() {
                admit(n, true)?;
            }
        }
        "ancestor" | "ancestor-or-self" => {
            let mut n = if axis == "ancestor" {
                context.parent()
            } else {
                Some(context)
            };
            while let Some(a) = n {
                admit(a, true)?;
                n = a.parent();
            }
            if !proximity {
                out.reverse();
            }
        }
        "child" => {
            if let SourceNode::Node(n) = context {
                for child in n.children() {
                    admit(SourceNode::Node(child), true)?;
                }
            }
        }
        "attribute" => {
            if let SourceNode::Node(n) = context {
                for a in n.attributes() {
                    admit(SourceNode::Attribute(a), true)?;
                }
            }
        }
        "namespace" => {
            // Source namespace iteration performs bounded ancestor/name lookups internally.
            // Conservatively reserve quadratic input work before entering that API.
            let bytes = doc.original_bytes().len();
            meter.work(
                bytes
                    .checked_mul(bytes)
                    .ok_or_else(crate::error::size_overflow)?,
            )?;
            if let SourceNode::Node(n) = context {
                for a in n.namespaces() {
                    meter.work(1)?;
                    let a = SourceNode::Namespace(a);
                    if matches(a, test, axis) {
                        meter.push(&mut out, a)?;
                    }
                }
            }
        }
        "preceding-sibling" | "following-sibling" => {
            if let SourceNode::Node(n) = context {
                if let Some(parent) = n.parent() {
                    for sibling in parent.children() {
                        let order = sibling.compare_order(n);
                        // Every visited sibling is charged, including rejected ones.
                        // Admission itself accounts the selected-node test.

                        let selected = (axis == "preceding-sibling"
                            && order == Some(std::cmp::Ordering::Less))
                            || (axis == "following-sibling"
                                && order == Some(std::cmp::Ordering::Greater));
                        admit(SourceNode::Node(sibling), selected)?;
                    }
                }
            }
            if axis == "preceding-sibling" && proximity {
                out.reverse();
            }
        }
        "descendant" | "descendant-or-self" => {
            if axis == "descendant-or-self" {
                admit(context, true)?;
            }
            if let SourceNode::Node(n) = context {
                // Explicit traversal with charged stack; no recursive source walk.
                let mut stack = Vec::new();
                for c in n.children().rev() {
                    meter.depth(
                        stack
                            .len()
                            .checked_add(1)
                            .ok_or_else(crate::error::size_overflow)?,
                    )?;
                    meter.push(&mut stack, c)?;
                }
                while let Some(c) = stack.pop() {
                    meter.release(size_of::<Node<'_>>());
                    meter.work(1)?;
                    if let Some(name) = c.name() {
                        meter.work(
                            name.local()
                                .len()
                                .checked_add(name.namespace_uri().len())
                                .ok_or_else(crate::error::size_overflow)?,
                        )?;
                    }
                    if matches(SourceNode::Node(c), test, axis) {
                        meter.push(&mut out, SourceNode::Node(c))?;
                    }
                    for child in c.children().rev() {
                        meter.depth(
                            stack
                                .len()
                                .checked_add(1)
                                .ok_or_else(crate::error::size_overflow)?,
                        )?;
                        meter.push(&mut stack, child)?;
                    }
                }
            }
        }
        _ => return Err(Error::Fatal(Failure::Internal("unprepared axis"))),
    }
    Ok(out)
}
