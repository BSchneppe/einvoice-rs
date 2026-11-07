use crate::decode::Coordinates;
use crate::error::Result;
use crate::{QueryBudget, SourceLimits};
use std::cmp::Ordering;
use std::sync::{atomic::AtomicBool, Arc};

/// Original encoding; UTF-16 imports require their corresponding BOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// UTF-8, optionally BOM-marked.
    Utf8,
    /// BOM-marked little-endian UTF-16.
    Utf16Le,
    /// BOM-marked big-endian UTF-16.
    Utf16Be,
    /// Declared ISO-8859-1/Latin1.
    Latin1,
    /// Declared US-ASCII/ASCII, strictly decoded.
    Ascii,
}
/// A half-open byte range into actual original UTF-8 bytes, including BOM offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalUtf8Span {
    /// Inclusive start.
    pub start: usize,
    /// Exclusive end.
    pub end: usize,
}
/// A half-open original decoded scalar range, before normalization, excluding BOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalScalarSpan {
    /// Inclusive start (a supplementary character counts once).
    pub start: usize,
    /// Exclusive end (CRLF counts twice and references retain their spelling).
    pub end: usize,
}
/// Public source evidence never exposes a transcoded UTF-8 byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSpan {
    /// Only for an actual original UTF-8 encoding.
    Utf8Byte(OriginalUtf8Span),
    /// For UTF-16/Latin1/ASCII original sources.
    UnicodeScalar(OriginalScalarSpan),
}
/// One-based original XML line/column. CRLF is one diagnostic newline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    /// One-based line.
    pub line: u64,
    /// One-based Unicode scalar column, excluding BOM.
    pub column: u64,
}
/// Internal decoded UTF-8 indexes cannot be confused with exported evidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DecodedSpan {
    pub start: usize,
    pub end: usize,
}
impl From<std::ops::Range<usize>> for DecodedSpan {
    fn from(r: std::ops::Range<usize>) -> Self {
        Self {
            start: r.start,
            end: r.end,
        }
    }
}
impl DecodedSpan {
    pub fn range(self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

/// Expanded name plus the exact lexical prefix/local name of this occurrence.
#[derive(Debug)]
pub struct Name {
    pub(crate) prefix: Option<String>,
    pub(crate) local: String,
    pub(crate) uri: String,
}
impl Name {
    /// Original lexical prefix, absent for unprefixed names.
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }
    /// Local name.
    pub fn local(&self) -> &str {
        &self.local
    }
    /// Expanded namespace URI (empty means no namespace).
    pub fn namespace_uri(&self) -> &str {
        &self.uri
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RawName {
    pub prefix: DecodedSpan,
    pub local: DecodedSpan,
}
/// Ordinary child-axis node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// The document node.
    Document,
    /// An element.
    Element,
    /// Nonempty merged adjacent text/CDATA/reference content.
    Text,
    /// A comment.
    Comment,
    /// A processing instruction, excluding the XML declaration.
    ProcessingInstruction,
}
#[derive(Debug)]
pub(crate) struct Record {
    pub kind: NodeKind,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub name: Option<Name>,
    pub raw_name: RawName,
    pub value: String,
    pub attributes: Vec<AttrRecord>,
    pub declarations: Vec<DeclRecord>,
    pub pieces: Vec<u32>,
    pub covering: DecodedSpan,
    pub occurrence: u32,
    pub subtree_end: u32,
    pub admitted_value: usize,
}
impl Record {
    pub fn new(kind: NodeKind, parent: Option<u32>, covering: DecodedSpan) -> Self {
        Self {
            kind,
            parent,
            children: Vec::new(),
            name: None,
            raw_name: RawName::default(),
            value: String::new(),
            attributes: Vec::new(),
            declarations: Vec::new(),
            pieces: Vec::new(),
            covering,
            occurrence: 1,
            subtree_end: 0,
            admitted_value: 0,
        }
    }
}
#[derive(Debug)]
pub(crate) struct AttrRecord {
    pub name: Option<Name>,
    pub raw_name: RawName,
    pub value: String,
    pub token: u32,
    pub value_span: DecodedSpan,
}
#[derive(Debug)]
pub(crate) struct DeclRecord {
    pub prefix: DecodedSpan,
    pub uri: String,
    pub token: u32,
    pub value_span: DecodedSpan,
}
/// Every lexical token, including material absent from the semantic child axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerKind {
    /// Original BOM marker; its decoded range is empty.
    Bom,
    /// XML declaration.
    Declaration,
    /// Opening element name/delimiter.
    ElementStart,
    /// Opening delimiter end (`>` or `/>`).
    StartTagEnd,
    /// Closing element tag.
    ElementEnd,
    /// Ordinary attribute (exact token and value span).
    Attribute,
    /// A namespace declaration (including redundant/default undeclaration).
    NamespaceDeclaration,
    /// Character data/reference spellings.
    Text,
    /// CDATA, including empty CDATA.
    Cdata,
    /// Comment.
    Comment,
    /// Processing instruction.
    ProcessingInstruction,
    /// Whitespace outside the document element, lexical only.
    DocumentWhitespace,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum Owner {
    Node(u32),
    Attribute(u32, u32),
    Declaration(u32, u32),
}
#[derive(Debug)]
pub(crate) struct Lexeme {
    pub kind: LedgerKind,
    pub span: DecodedSpan,
    pub value: Option<DecodedSpan>,
    pub owner: Owner,
}

/// Immutable owned source. Handles borrow this allocation and keep ordinals private.
/// Arc clones share document identity; each successful parse receives a fresh ID.
#[derive(Debug)]
pub struct SourceDocument {
    pub(crate) identity: u64,
    pub(crate) raw: Arc<[u8]>,
    pub(crate) decoded: String,
    pub(crate) encoding: Encoding,
    pub(crate) bom: usize,
    pub(crate) coordinates: Coordinates,
    pub(crate) records: Vec<Record>,
    pub(crate) ledger: Vec<Lexeme>,
}
impl SourceDocument {
    /// Parse once with default construction limits; this does not validate invoices.
    pub fn parse(bytes: &[u8]) -> Result<Arc<Self>> {
        Self::parse_with_limits(bytes, &SourceLimits::default())
    }
    /// Construct under explicit finite limits.
    pub fn parse_with_limits(bytes: &[u8], limits: &SourceLimits) -> Result<Arc<Self>> {
        crate::parse::parse(bytes, limits, None)
    }
    /// Construct with cooperative cancellation checkpoints (not a hard deadline).
    pub fn parse_cancellable(
        bytes: &[u8],
        limits: &SourceLimits,
        cancel: &AtomicBool,
    ) -> Result<Arc<Self>> {
        crate::parse::parse(bytes, limits, Some(cancel))
    }
    /// Exact original bytes; never regenerated from semantic nodes.
    pub fn original_bytes(&self) -> &[u8] {
        &self.raw
    }
    /// Non-lossy original decoded text, BOM excluded, before normalization.
    pub fn source_view(&self) -> &str {
        &self.decoded
    }
    /// Detected and strictly checked original encoding.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
    /// Original BOM byte count.
    pub fn bom_bytes(&self) -> usize {
        self.bom
    }
    /// The document node.
    pub fn document(&self) -> Node<'_> {
        Node { doc: self, id: 0 }
    }
    /// The unique document element.
    pub fn root_element(&self) -> Node<'_> {
        self.document()
            .children()
            .find(|n| n.kind() == NodeKind::Element)
            .expect("checked document element")
    }
    /// Preorder ordinary nodes, including document-level comments/PIs.
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = Node<'_>> + DoubleEndedIterator {
        (0..self.records.len()).map(|id| Node {
            doc: self,
            id: id as u32,
        })
    }
    /// Complete token ledger; gaps between tokens remain accessible.
    pub fn ledger(&self) -> impl ExactSizeIterator<Item = LedgerEntry<'_>> + DoubleEndedIterator {
        (0..self.ledger.len()).map(|id| LedgerEntry {
            doc: self,
            id: id as u32,
        })
    }
    /// Remaining original decoded gap after the last token.
    pub fn trailing_gap(&self) -> &str {
        &self.decoded[self.ledger.last().map_or(0, |l| l.span.end)..]
    }
    pub(crate) fn span(&self, r: DecodedSpan) -> EvidenceSpan {
        if self.encoding == Encoding::Utf8 {
            EvidenceSpan::Utf8Byte(OriginalUtf8Span {
                start: r.start + self.bom,
                end: r.end + self.bom,
            })
        } else {
            EvidenceSpan::UnicodeScalar(OriginalScalarSpan {
                start: self.coordinates.scalar(&self.decoded, r.start),
                end: self.coordinates.scalar(&self.decoded, r.end),
            })
        }
    }
    pub(crate) fn location(&self, at: usize) -> SourceLocation {
        self.coordinates.location(&self.decoded, at)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LocalIdentity {
    Node(u32),
    Attribute(u32, u32),
    Namespace(u32, u32),
}
/// Opaque identity includes a unique parsed-document ID and a typed local ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIdentity {
    document: u64,
    local: LocalIdentity,
}
/// A document-scoped borrowed ordinary node; local ordinals cannot be imported.
#[derive(Debug, Clone, Copy)]
pub struct Node<'a> {
    pub(crate) doc: &'a SourceDocument,
    pub(crate) id: u32,
}
impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}
impl Eq for Node<'_> {}
impl<'a> Node<'a> {
    pub(crate) fn record(self) -> &'a Record {
        &self.doc.records[self.id as usize]
    }
    /// Stable identity for the lifetime of this parsed document and beyond.
    pub fn identity(self) -> NodeIdentity {
        NodeIdentity {
            document: self.doc.identity,
            local: LocalIdentity::Node(self.id),
        }
    }
    /// Node kind.
    pub fn kind(self) -> NodeKind {
        self.record().kind
    }
    /// Expanded and lexical name (PI target is its local name, no namespace).
    pub fn name(self) -> Option<&'a Name> {
        self.record().name.as_ref()
    }
    /// Borrowed text/comment/PI content. Elements/documents have no stored string value.
    pub fn content(self) -> Option<&'a str> {
        matches!(
            self.kind(),
            NodeKind::Text | NodeKind::Comment | NodeKind::ProcessingInstruction
        )
        .then_some(self.record().value.as_str())
    }
    /// Parent on the ordinary child axis.
    pub fn parent(self) -> Option<Self> {
        self.record().parent.map(|id| Self { doc: self.doc, id })
    }
    /// Children in source document order; attributes/namespaces are separate axes.
    pub fn children(self) -> Children<'a> {
        Children {
            doc: self.doc,
            ids: self.record().children.iter(),
        }
    }
    /// Ordinary attributes in exact source order.
    pub fn attributes(self) -> impl ExactSizeIterator<Item = Attribute<'a>> + DoubleEndedIterator {
        (0..self.record().attributes.len()).map(move |index| Attribute {
            owner: self,
            index: index as u32,
        })
    }
    /// Local namespace declarations, retaining aliases/redundancy/unbinding.
    pub fn declarations(self) -> impl ExactSizeIterator<Item = Declaration<'a>> {
        (0..self.record().declarations.len()).map(move |index| Declaration {
            owner: self,
            index: index as u32,
        })
    }
    /// Persistent namespace view: nearest frame first, source order within frame,
    /// then ancestors, then implicit xml. Shadowed/empty/xmlns bindings excluded.
    pub fn namespaces(self) -> Namespaces<'a> {
        Namespaces {
            owner: self,
            frame: Some(self),
            index: 0,
            ordinal: 0,
            implicit: false,
        }
    }
    pub(crate) fn binding(self, prefix: &str) -> Option<(u32, u32)> {
        let mut frame = Some(self);
        while let Some(n) = frame {
            for (index, d) in n.record().declarations.iter().enumerate() {
                if &self.doc.decoded[d.prefix.range()] == prefix {
                    return Some((n.id, index as u32));
                }
            }
            frame = n.parent();
        }
        None
    }
    /// Cached sibling occurrence by expanded element name, kind, or PI target.
    pub fn occurrence(self) -> u32 {
        self.record().occurrence
    }
    /// Covering source range; for text it is NOT claimed to be a single token.
    pub fn covering_span(self) -> EvidenceSpan {
        self.doc.span(self.record().covering)
    }
    /// Original source position of the covering range start.
    pub fn location(self) -> SourceLocation {
        self.doc.location(self.record().covering.start)
    }
    /// Ordered contributing lexical pieces. Empty CDATA is associated with its
    /// owner element unless it occurs in an existing text run.
    pub fn pieces(self) -> impl ExactSizeIterator<Item = LedgerEntry<'a>> {
        self.record().pieces.iter().map(move |id| LedgerEntry {
            doc: self.doc,
            id: *id,
        })
    }
    /// Ordinary node order, comparable only within this parsed document.
    pub fn compare_order(self, other: Node<'_>) -> Option<Ordering> {
        (self.doc.identity == other.doc.identity).then(|| self.id.cmp(&other.id))
    }
    /// Stream XPath string-value using explicit output/work budgets.
    pub fn write_string_value(
        self,
        writer: &mut impl std::fmt::Write,
        budget: &mut QueryBudget,
    ) -> Result<()> {
        crate::render::value(self, writer, budget)
    }
    /// Collect XPath string-value under explicit output/work budgets.
    pub fn string_value(self, budget: &mut QueryBudget) -> Result<String> {
        crate::render::collect_value(self, budget)
    }
    /// Collect schxslt-compatible occurrence path under explicit budgets.
    pub fn canonical_path(self, budget: &mut QueryBudget) -> Result<String> {
        crate::render::path(self, None, budget)
    }
    /// Order key: element, namespaces, attributes, then ordinary children.
    /// Keys from distinct documents are deliberately incomparable.
    pub fn order(self) -> DocumentOrder {
        DocumentOrder {
            document: self.doc.identity,
            node: self.id,
            axis: 0,
            ordinal: 0,
        }
    }
}
/// An allocation-free child iterator.
#[derive(Clone)]
pub struct Children<'a> {
    doc: &'a SourceDocument,
    ids: std::slice::Iter<'a, u32>,
}
impl<'a> Iterator for Children<'a> {
    type Item = Node<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.ids.next().map(|id| Node {
            doc: self.doc,
            id: *id,
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ids.size_hint()
    }
}
impl DoubleEndedIterator for Children<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.ids.next_back().map(|id| Node {
            doc: self.doc,
            id: *id,
        })
    }
}
impl ExactSizeIterator for Children<'_> {}
/// A source-order ordinary attribute handle, distinct from a child node.
#[derive(Debug, Clone, Copy)]
pub struct Attribute<'a> {
    pub(crate) owner: Node<'a>,
    pub(crate) index: u32,
}
impl<'a> Attribute<'a> {
    fn record(self) -> &'a AttrRecord {
        &self.owner.record().attributes[self.index as usize]
    }
    /// Owning element.
    pub fn owner(self) -> Node<'a> {
        self.owner
    }
    /// Expanded/lexical attribute name.
    pub fn name(self) -> &'a Name {
        self.record().name.as_ref().expect("checked attribute name")
    }
    /// Normalized, expanded attribute value.
    pub fn value(self) -> &'a str {
        &self.record().value
    }
    /// Exact attribute token (not the owner's fallback range).
    pub fn token(self) -> LedgerEntry<'a> {
        LedgerEntry {
            doc: self.owner.doc,
            id: self.record().token,
        }
    }
    /// Exact original value range, excluding quotes.
    pub fn value_span(self) -> EvidenceSpan {
        self.owner.doc.span(self.record().value_span)
    }
    /// Explicit fallback for consumers lacking attribute evidence support.
    pub fn owner_fallback_span(self) -> EvidenceSpan {
        self.owner.covering_span()
    }
    /// Typed owner-qualified identity.
    pub fn identity(self) -> NodeIdentity {
        NodeIdentity {
            document: self.owner.doc.identity,
            local: LocalIdentity::Attribute(self.owner.id, self.index),
        }
    }
    /// Attribute order follows all effective namespaces.
    pub fn order(self) -> DocumentOrder {
        DocumentOrder {
            document: self.owner.doc.identity,
            node: self.owner.id,
            axis: 2,
            ordinal: self.index,
        }
    }
    /// Budgeted canonical attribute path.
    pub fn canonical_path(self, budget: &mut QueryBudget) -> Result<String> {
        crate::render::path(self.owner, Some(self), budget)
    }
}
/// A local lexical namespace declaration, distinct from a namespace-axis node.
#[derive(Debug, Clone, Copy)]
pub struct Declaration<'a> {
    owner: Node<'a>,
    index: u32,
}
impl<'a> Declaration<'a> {
    fn record(self) -> &'a DeclRecord {
        &self.owner.record().declarations[self.index as usize]
    }
    /// Declared prefix; empty denotes the default namespace.
    pub fn prefix(self) -> &'a str {
        &self.owner.doc.decoded[self.record().prefix.range()]
    }
    /// Normalized expanded URI, including empty default undeclarations.
    pub fn uri(self) -> &'a str {
        &self.record().uri
    }
    /// Exact lexical token.
    pub fn token(self) -> LedgerEntry<'a> {
        LedgerEntry {
            doc: self.owner.doc,
            id: self.record().token,
        }
    }
    /// Exact original declaration value range.
    pub fn value_span(self) -> EvidenceSpan {
        self.owner.doc.span(self.record().value_span)
    }
    /// Owning element.
    pub fn owner(self) -> Node<'a> {
        self.owner
    }
}
/// An effective namespace node belongs to the element exposing the namespace axis.
#[derive(Debug, Clone, Copy)]
pub struct Namespace<'a> {
    owner: Node<'a>,
    declaration: Option<Declaration<'a>>,
    ordinal: u32,
}
impl<'a> Namespace<'a> {
    /// Prefix, including implicit xml.
    pub fn prefix(self) -> &'a str {
        self.declaration.map_or("xml", |d| d.prefix())
    }
    /// Effective URI.
    pub fn uri(self) -> &'a str {
        self.declaration
            .map_or("http://www.w3.org/XML/1998/namespace", |d| d.uri())
    }
    /// Element owning this namespace-axis node.
    pub fn owner(self) -> Node<'a> {
        self.owner
    }
    /// Original declaration if explicitly present in a persistent ancestor frame.
    pub fn declaration(self) -> Option<Declaration<'a>> {
        self.declaration
    }
    /// Typed namespace identity, qualified by the exposing element.
    pub fn identity(self) -> NodeIdentity {
        NodeIdentity {
            document: self.owner.doc.identity,
            local: LocalIdentity::Namespace(self.owner.id, self.ordinal),
        }
    }
    /// Stable implementation namespace order, before ordinary attributes.
    pub fn order(self) -> DocumentOrder {
        DocumentOrder {
            document: self.owner.doc.identity,
            node: self.owner.id,
            axis: 1,
            ordinal: self.ordinal,
        }
    }
}
/// Allocation-free iteration over persistent bounded namespace frames.
pub struct Namespaces<'a> {
    owner: Node<'a>,
    frame: Option<Node<'a>>,
    index: usize,
    ordinal: u32,
    implicit: bool,
}
impl<'a> Iterator for Namespaces<'a> {
    type Item = Namespace<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.owner.kind() != NodeKind::Element {
            return None;
        }
        while let Some(frame) = self.frame {
            while self.index < frame.record().declarations.len() {
                let index = self.index as u32;
                self.index += 1;
                let d = Declaration {
                    owner: frame,
                    index,
                };
                if !d.uri().is_empty()
                    && d.prefix() != "xmlns"
                    && self.owner.binding(d.prefix()) == Some((frame.id, index))
                {
                    let ordinal = self.ordinal;
                    self.ordinal += 1;
                    return Some(Namespace {
                        owner: self.owner,
                        declaration: Some(d),
                        ordinal,
                    });
                }
            }
            self.frame = frame.parent();
            self.index = 0;
        }
        if !self.implicit {
            self.implicit = true;
            if self.owner.binding("xml").is_none() {
                let ordinal = self.ordinal;
                self.ordinal += 1;
                return Some(Namespace {
                    owner: self.owner,
                    declaration: None,
                    ordinal,
                });
            }
        }
        None
    }
}
/// An owner-qualified document order key. No arbitrary cross-document ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentOrder {
    document: u64,
    node: u32,
    axis: u8,
    ordinal: u32,
}
impl PartialOrd for DocumentOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (self.document == other.document).then(|| {
            (self.node, self.axis, self.ordinal).cmp(&(other.node, other.axis, other.ordinal))
        })
    }
}
/// Owner association for every lexical token, without exposed local IDs.
#[derive(Debug, Clone, Copy)]
pub enum LedgerOwner<'a> {
    /// Ordinary owner or contributing text node.
    Node(Node<'a>),
    /// Exact ordinary attribute.
    Attribute(Attribute<'a>),
    /// Exact local namespace declaration.
    Declaration(Declaration<'a>),
}
/// Borrowed complete lexical token evidence.
#[derive(Debug, Clone, Copy)]
pub struct LedgerEntry<'a> {
    doc: &'a SourceDocument,
    id: u32,
}
impl<'a> LedgerEntry<'a> {
    fn record(self) -> &'a Lexeme {
        &self.doc.ledger[self.id as usize]
    }
    /// Token category.
    pub fn kind(self) -> LedgerKind {
        self.record().kind
    }
    /// Original decoded token spelling (BOM represented by its metadata kind).
    pub fn source(self) -> &'a str {
        &self.doc.decoded[self.record().span.range()]
    }
    /// Original decoded gap immediately preceding this token.
    pub fn gap_before(self) -> &'a str {
        let start = if self.id == 0 {
            0
        } else {
            self.doc.ledger[self.id as usize - 1].span.end
        };
        &self.doc.decoded[start..self.record().span.start]
    }
    /// Exact public evidence range.
    pub fn span(self) -> EvidenceSpan {
        if self.kind() == LedgerKind::Bom && self.doc.encoding == Encoding::Utf8 {
            EvidenceSpan::Utf8Byte(OriginalUtf8Span {
                start: 0,
                end: self.doc.bom,
            })
        } else {
            self.doc.span(self.record().span)
        }
    }
    /// Exact value/content range where the tokenizer supplies one.
    pub fn value_span(self) -> Option<EvidenceSpan> {
        self.record().value.map(|r| self.doc.span(r))
    }
    /// Original source position.
    pub fn location(self) -> SourceLocation {
        self.doc.location(self.record().span.start)
    }
    /// Checked owner association.
    pub fn owner(self) -> LedgerOwner<'a> {
        match self.record().owner {
            Owner::Node(id) => LedgerOwner::Node(Node { doc: self.doc, id }),
            Owner::Attribute(id, index) => LedgerOwner::Attribute(Attribute {
                owner: Node { doc: self.doc, id },
                index,
            }),
            Owner::Declaration(id, index) => LedgerOwner::Declaration(Declaration {
                owner: Node { doc: self.doc, id },
                index,
            }),
        }
    }
}
