//! The well-formedness layer over quick-xml.
//!
//! [`WfReader`] wraps a `quick_xml::Reader` and checks every event it hands
//! out before the caller sees it: the document structure, names, tags,
//! characters, references, the XML and document type declarations, the
//! namespace constraints (see the [module documentation](crate::xml)), and
//! the [`ParseLimits`]. A start tag is checked as a whole — every attribute,
//! every namespace declaration, every prefix, reference and byte — before it
//! is returned, so nothing of a refused tag reaches a caller that interns
//! names. What quick-xml itself detects (mismatched end tags, unclosed
//! markup) is reported the same way, as [`NotWellFormed`]; only an I/O error
//! of the source stays quick-xml's.
//!
//! The rules are those of the version the XML declaration names: XML 1.1
//! (Second Edition) and Namespaces in XML 1.1 for exactly `version="1.1"`,
//! XML 1.0 (Fifth Edition) and Namespaces in XML 1.0 otherwise. The 1.0 scans
//! are the hot loops; the 1.1 ones are separate functions.
//!
//! The checks run over bytes the event already holds: one pass over a start
//! tag, one over a text run, comment, CDATA section or processing
//! instruction, each with a vectorizable fast path for the common case.
//!
//! Positions are byte offsets into the input, a leading byte-order mark
//! included ([`buffer_position`](WfReader::buffer_position) too).
//!
//! The case of the `<!DOCTYPE` keyword is not visible in quick-xml's event:
//! it is checked on the markup still in the buffer, by
//! [`confirm_doctype`](WfReader::confirm_doctype) (a caller that reports the
//! declaration calls it first) or else at the next read.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::io::BufRead;

use ahash::AHasher;
use quick_xml::errors::{IllFormedError, SyntaxError};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::chars::{
    char_ref_value, decode_at, find_bad_utf8, find_non_char, find_non_char11, is_char, is_char11,
    is_name, is_name_char, is_name_start, is_space, scan_name, xml11_line_end_at, NameScan,
};
use super::error::{NotWellFormed, WellFormednessRule as R, XmlDeclaration, XmlVersion};
use super::limits::{LimitExceeded, ParseLimit, ParseLimits};
use crate::namespace::table::XML_NAMESPACE;

/// The namespace name bound to the `xmlns` prefix (Namespaces in XML 1.0
/// §3).
pub(crate) const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<AHasher>>;

/// Why a [`WfReader`] stopped.
#[derive(Debug)]
pub(crate) enum WfError {
    /// An I/O error of the source — the one thing the reader reports that
    /// no well-formedness rule names.
    Io(std::io::Error),
    /// A well-formedness rule.
    NotWellFormed(NotWellFormed),
    /// A parse limit.
    Limit(LimitExceeded),
}

/// Where a caller decodes the event a [`WfReader`] returned last (its start,
/// as an input offset, and the version).
///
/// The layer checks every event before it returns it: its names, attribute
/// values and character data are well-formed UTF-8, its prefixes bound, its
/// references to characters or to the five predefined entities, and the
/// input does not end inside an element. A caller that still finds one of
/// these wrong while decoding the event — an invariant of the layer broken —
/// reports it through these as the well-formedness error it is, not as a
/// panic and not as a quick-xml error. Not reached for an event the layer
/// returned.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Recheck {
    pos: u64,
    version: XmlVersion,
}

impl Recheck {
    /// Bytes that are not UTF-8 (XML 1.0 §4.3.3).
    #[cold]
    #[inline(never)]
    pub(crate) fn utf8(self) -> WfError {
        self.refusal(R::Encoding, "bytes that are not UTF-8")
    }

    /// A prefix with no namespace in scope (NSC: Prefix Declared).
    #[cold]
    #[inline(never)]
    pub(crate) fn unbound(self, prefix: &[u8]) -> WfError {
        self.refusal(
            R::PrefixDeclared,
            format!(
                "the prefix '{}' is not bound to a namespace in scope",
                shown(prefix)
            ),
        )
    }

    /// Input that ends inside `depth` elements.
    #[cold]
    #[inline(never)]
    pub(crate) fn truncated(self, depth: usize) -> WfError {
        self.refusal(
            R::Truncated,
            format!(
                "the input ends inside {depth} element{} not closed",
                if depth == 1 { "" } else { "s" }
            ),
        )
    }

    /// What a quick-xml decoding or normalization function reports.
    #[cold]
    #[inline(never)]
    pub(crate) fn decoding(self, e: quick_xml::Error) -> WfError {
        decoding_refusal(e, self.pos, self.version)
    }

    fn refusal(self, rule: R, message: impl Into<Box<str>>) -> WfError {
        WfError::NotWellFormed(NotWellFormed::new(rule, self.pos, message, self.version))
    }
}

/// A quick-xml error as the layer reports it, at the input offset `pos`: the
/// I/O error of the source as [`WfError::Io`], everything else as the
/// well-formedness rule it breaks.
#[cold]
#[inline(never)]
fn decoding_refusal(e: quick_xml::Error, pos: u64, version: XmlVersion) -> WfError {
    match e {
        // quick-xml wraps the source's error in a fresh `Arc` to make its own
        // error type `Clone`, and holds no other reference, so the error comes
        // out as it was; were it shared, it is kept as the source of an error
        // of the same kind.
        quick_xml::Error::Io(e) => WfError::Io(
            std::sync::Arc::try_unwrap(e)
                .unwrap_or_else(|shared| std::io::Error::new(shared.kind(), shared)),
        ),
        other => WfError::NotWellFormed(NotWellFormed::new(
            rule_of(&other),
            pos,
            other.to_string(),
            version,
        )),
    }
}

/// The well-formedness rule a quick-xml error (other than I/O) reports.
fn rule_of(e: &quick_xml::Error) -> R {
    use quick_xml::escape::EscapeError;
    use quick_xml::events::attributes::AttrError;
    use quick_xml::name::NamespaceError;
    match e {
        // Not a refusal; `decoding_refusal` takes it apart first.
        quick_xml::Error::Io(_) => R::Markup,
        quick_xml::Error::Syntax(_) => R::Truncated,
        quick_xml::Error::IllFormed(_) => R::Markup,
        quick_xml::Error::Encoding(_) => R::Encoding,
        quick_xml::Error::InvalidAttr(AttrError::Duplicated(..)) => R::UniqueAttSpec,
        quick_xml::Error::InvalidAttr(_) => R::StartTag,
        quick_xml::Error::Escape(EscapeError::UnrecognizedEntity(..)) => R::EntityDeclared,
        quick_xml::Error::Escape(EscapeError::InvalidCharRef(_)) => R::LegalCharacter,
        quick_xml::Error::Escape(_) => R::Reference,
        quick_xml::Error::Namespace(NamespaceError::UnknownPrefix(_)) => R::PrefixDeclared,
        quick_xml::Error::Namespace(_) => R::ReservedPrefix,
    }
}

/// Where the reader is in the document (\[1\] `document ::= prolog element
/// Misc*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Prolog,
    Root,
    Epilog,
}

/// The limits, `usize::MAX` for none.
#[derive(Debug, Clone, Copy)]
struct Max {
    depth: usize,
    name: usize,
    uri: usize,
    attributes: usize,
    declarations: usize,
    in_scope: usize,
    value: usize,
    text: usize,
    nodes: usize,
}

impl Max {
    fn of(l: &ParseLimits) -> Self {
        let m = |v: Option<usize>| v.unwrap_or(usize::MAX);
        Max {
            depth: m(l.max_depth),
            name: m(l.max_name_bytes),
            uri: m(l.max_namespace_uri_bytes),
            attributes: m(l.max_attributes),
            declarations: m(l.max_namespace_declarations),
            in_scope: m(l.max_namespaces_in_scope),
            value: m(l.max_attribute_value_bytes),
            text: m(l.max_text_bytes),
            nodes: m(l.max_nodes),
        }
    }
}

/// One namespace binding in scope: the namespace name and its hash, computed
/// once per declaration, so that attributes compare expanded names by hash
/// first. An empty name is a prefix undeclared (Namespaces in XML 1.1).
#[derive(Debug)]
struct Binding {
    hash: u64,
    uri: Box<[u8]>,
}

/// The layer's own namespace scope: what each prefix is bound to, for NSC:
/// Prefix Declared, NSC: Attributes Unique and the namespaces-in-scope limit.
/// Changed only by elements that declare namespaces.
#[derive(Debug, Default)]
struct Scope {
    /// Per prefix (the empty one for the default namespace), its bindings
    /// from the outermost declaration in force to the innermost: the
    /// restoration stack. An empty name is an undeclaration.
    bindings: FastMap<Box<[u8]>, Vec<Binding>>,
    /// The prefixes declared by the open elements, in order.
    declared: Vec<Box<[u8]>>,
    /// Per open element with declarations: its depth and `declared.len()`
    /// before them.
    marks: Vec<(usize, usize)>,
    /// How many prefixes (the default namespace included, the `xml` prefix
    /// not) are bound now — whose innermost binding is not an
    /// undeclaration.
    active: usize,
}

impl Scope {
    /// The prefixes bound now (see `active`).
    fn in_scope(&self) -> usize {
        self.active
    }

    /// Whether `prefix` counts towards `active` with this innermost binding.
    fn counts(prefix: &[u8], top: Option<&Binding>) -> bool {
        prefix != b"xml" && top.is_some_and(|b| !b.uri.is_empty())
    }

    fn push(&mut self, depth: usize, prefix: &[u8], uri: &[u8]) {
        if self.marks.last().map(|m| m.0) != Some(depth) {
            self.marks.push((depth, self.declared.len()));
        }
        let binding = Binding {
            hash: fnv1a(uri),
            uri: uri.into(),
        };
        let was = match self.bindings.get_mut(prefix) {
            Some(stack) => {
                let was = Self::counts(prefix, stack.last());
                stack.push(binding);
                was
            }
            None => {
                self.bindings.insert(prefix.into(), vec![binding]);
                false
            }
        };
        let now = prefix != b"xml" && !uri.is_empty();
        self.active = self.active + usize::from(now) - usize::from(was);
        self.declared.push(prefix.into());
    }

    /// Closes the element at `depth`.
    #[inline]
    fn pop(&mut self, depth: usize) {
        if self.marks.last().map(|m| m.0) != Some(depth) {
            return;
        }
        self.pop_declarations();
    }

    #[cold]
    #[inline(never)]
    fn pop_declarations(&mut self) {
        let (_, mark) = self.marks.pop().expect("checked above");
        while self.declared.len() > mark {
            let prefix = self.declared.pop().expect("above the mark");
            if let Some(stack) = self.bindings.get_mut(&prefix) {
                let was = Self::counts(&prefix, stack.last());
                stack.pop();
                let now = Self::counts(&prefix, stack.last());
                self.active = self.active + usize::from(now) - usize::from(was);
                if stack.is_empty() {
                    self.bindings.remove(&prefix);
                }
            }
        }
    }

    /// The namespace a prefix is bound to: `(hash, name)`, or `None` when it
    /// is not bound (never declared, or undeclared in XML 1.1).
    fn resolve(&self, prefix: &[u8]) -> Option<(u64, &[u8])> {
        if prefix == b"xml" {
            return Some((fnv1a(XML_NAMESPACE.as_bytes()), XML_NAMESPACE.as_bytes()));
        }
        self.bindings
            .get(prefix)
            .and_then(|s| s.last())
            .filter(|b| !b.uri.is_empty())
            .map(|b| (b.hash, &*b.uri))
    }
}

fn fnv1a(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// What an attribute of a start tag is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrKind {
    /// An attribute.
    Attribute,
    /// `xmlns="…"`.
    DefaultNamespace,
    /// `xmlns:p="…"`.
    PrefixNamespace,
}

/// One attribute of the start tag [`WfReader::read_event_into`] returned
/// last, as offsets into the tag's bytes (`BytesStart`'s, between `<` and
/// `>`), so a caller reads the tag's attributes without parsing it again.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AttrSpan {
    key: (usize, usize),
    /// The offset of the colon in the name, for a prefixed attribute.
    colon: Option<usize>,
    value: (usize, usize),
    pub(crate) kind: AttrKind,
    /// The value needs attribute-value normalization (it holds a reference,
    /// a tab, a line feed, a carriage return, or an XML 1.1 line end).
    needs_normalization: bool,
}

impl AttrSpan {
    /// The qualified name.
    pub(crate) fn key<'t>(&self, tag: &'t [u8]) -> &'t [u8] {
        &tag[self.key.0..self.key.1]
    }

    /// The prefix of an attribute (empty when unprefixed), or the prefix a
    /// namespace declaration declares (empty for the default namespace).
    pub(crate) fn prefix<'t>(&self, tag: &'t [u8]) -> &'t [u8] {
        match (self.kind, self.colon) {
            (AttrKind::Attribute, Some(c)) => &tag[self.key.0..c],
            (AttrKind::PrefixNamespace, Some(c)) => &tag[c + 1..self.key.1],
            _ => &[],
        }
    }

    /// The local part of an attribute's name.
    pub(crate) fn local<'t>(&self, tag: &'t [u8]) -> &'t [u8] {
        match self.colon {
            Some(c) => &tag[c + 1..self.key.1],
            None => self.key(tag),
        }
    }

    /// The value as written.
    pub(crate) fn raw_value<'t>(&self, tag: &'t [u8]) -> &'t [u8] {
        &tag[self.value.0..self.value.1]
    }

    /// The normalized value (XML 1.0 / 1.1 §3.3.3), as quick-xml's
    /// `Attribute::normalized_value` gives it for `version` (the reader's
    /// [`quick_xml_version`](WfReader::quick_xml_version)). The value was
    /// checked: references resolve, the bytes are UTF-8.
    pub(crate) fn value<'t>(
        &self,
        tag: &'t [u8],
        version: quick_xml::XmlVersion,
    ) -> Result<Cow<'t, str>, quick_xml::Error> {
        let raw = self.raw_value(tag);
        if self.needs_normalization {
            quick_xml::events::attributes::Attribute {
                key: quick_xml::name::QName(self.key(tag)),
                value: Cow::Borrowed(raw),
            }
            .normalized_value(version)
        } else {
            std::str::from_utf8(raw)
                .map(Cow::Borrowed)
                .map_err(|e| quick_xml::Error::Encoding(e.into()))
        }
    }
}

/// The document type declaration returned last, whose keyword is still to be
/// checked on its markup: where it ended, and its content's length and hash
/// (so that an unrelated buffer is not mistaken for it).
#[derive(Debug, Clone, Copy)]
struct PendingDoctype {
    after: u64,
    len: usize,
    hash: u64,
}

/// The well-formedness layer over a `quick_xml::Reader`.
pub(crate) struct WfReader<B> {
    inner: Reader<B>,
    /// Length of a leading UTF-8 byte-order mark that quick-xml removed (it
    /// counts offsets after it); added to every reported position.
    bom: u64,
    /// Whether the start of the input was looked at for a BOM.
    started: bool,
    /// Where an XML declaration may start, in quick-xml offsets: 0, or 3
    /// after a BOM quick-xml did not remove (one that arrived split across
    /// `fill_buf` calls).
    decl_at: u64,
    max: Max,
    phase: Phase,
    depth: usize,
    /// Events handed out so far (a BOM-only text event is not one).
    events: u64,
    count_event: bool,
    seen_doctype: bool,
    doctype_pending: Option<PendingDoctype>,
    nodes: usize,
    /// In a run of character data (text and references): its start and size
    /// so far.
    run: Option<(u64, usize)>,
    scope: Scope,
    /// The attributes of the last start tag.
    attrs: Vec<AttrSpan>,
    version: XmlVersion,
    declaration: Option<XmlDeclaration>,
    /// Where the last event's markup starts, in quick-xml offsets.
    last_start: u64,
    /// The qualified names of the open elements, as written, one after the
    /// other (`open_at` has where each starts): for naming the innermost one
    /// when the input ends inside it.
    open: Vec<u8>,
    open_at: Vec<usize>,
}

impl<B: BufRead> WfReader<B> {
    /// A reader over `reader` with `limits`. `trim_text_start` is quick-xml's
    /// option (white space before character data and markup is skipped); the
    /// instance readers keep it off, the schema-document reader on.
    pub(crate) fn new(reader: B, limits: &ParseLimits, trim_text_start: bool) -> Self {
        let mut inner = Reader::from_reader(reader);
        let config = inner.config_mut();
        config.trim_text_start = trim_text_start;
        config.trim_text_end = false;
        config.check_end_names = true;
        WfReader {
            inner,
            bom: 0,
            started: false,
            decl_at: 0,
            max: Max::of(limits),
            phase: Phase::Prolog,
            depth: 0,
            events: 0,
            count_event: true,
            seen_doctype: false,
            doctype_pending: None,
            nodes: 0,
            run: None,
            scope: Scope::default(),
            attrs: Vec::new(),
            version: XmlVersion::V1_0,
            declaration: None,
            last_start: 0,
            open: Vec::new(),
            open_at: Vec::new(),
        }
    }

    /// The attributes of the start tag returned last (offsets into its
    /// bytes), in document order.
    pub(crate) fn attributes(&self) -> &[AttrSpan] {
        &self.attrs
    }

    /// The underlying quick-xml reader.
    pub(crate) fn inner(&self) -> &Reader<B> {
        &self.inner
    }

    /// The byte offset into the input (a leading BOM included) up to which
    /// the input has been read.
    pub(crate) fn buffer_position(&self) -> u64 {
        self.inner.buffer_position() + self.bom
    }

    /// The byte offset into the input (a leading BOM included) where the
    /// markup of the event returned last starts: the `<` of a tag or a
    /// declaration.
    pub(crate) fn event_start(&self) -> u64 {
        self.last_start + self.bom
    }

    /// The version whose rules apply (known once the XML declaration, if
    /// any, has been read).
    pub(crate) fn xml_version(&self) -> XmlVersion {
        self.version
    }

    /// The same, as quick-xml's normalization functions take it.
    pub(crate) fn quick_xml_version(&self) -> quick_xml::XmlVersion {
        match self.version {
            XmlVersion::V1_1 => quick_xml::XmlVersion::Explicit1_1,
            _ => quick_xml::XmlVersion::Implicit1_0,
        }
    }

    /// The XML declaration, once it has been read.
    pub(crate) fn declaration(&self) -> Option<&XmlDeclaration> {
        self.declaration.as_ref()
    }

    /// Checks the keyword of the document type declaration just returned,
    /// on its markup in `buf` (the buffer it was read into, before the next
    /// read). A caller that reports the declaration calls this first.
    pub(crate) fn confirm_doctype(&mut self, buf: &[u8]) -> Result<(), WfError> {
        match self.doctype_pending.take() {
            Some(p) => self.check_doctype_markup(buf, p),
            None => Ok(()),
        }
    }

    /// The next event, checked. Clears `buf` first.
    pub(crate) fn read_event_into<'b>(
        &mut self,
        buf: &'b mut Vec<u8>,
    ) -> Result<Event<'b>, WfError> {
        if !self.started {
            self.started = true;
            // quick-xml removes a UTF-8 BOM it sees in the first fill of its
            // source and counts offsets after it; look the same way (a BOM
            // split across fills is taken care of in `check_text`).
            match self.inner.get_mut().fill_buf() {
                Ok([0xEF, 0xBB, 0xBF, ..]) => self.bom = 3,
                Ok(_) => {}
                Err(e) => return Err(WfError::Io(e)),
            }
        }
        if let Some(p) = self.doctype_pending.take() {
            self.check_doctype_markup(buf, p)?;
        }
        buf.clear();
        let before = self.inner.buffer_position();
        let event = match self.inner.read_event_into(buf) {
            Ok(event) => event,
            Err(e) => return Err(self.quick_xml_error(e)),
        };
        let after = self.inner.buffer_position();
        self.check(&event, before, after)?;
        if self.count_event {
            self.events += 1;
        } else {
            self.count_event = true;
        }
        Ok(event)
    }

    // ── Errors ───────────────────────────────────────────────────────────

    #[cold]
    #[inline(never)]
    fn nwf(&self, rule: R, pos: u64, message: impl Into<Box<str>>) -> WfError {
        WfError::NotWellFormed(NotWellFormed::new(
            rule,
            pos + self.bom,
            message,
            self.version,
        ))
    }

    #[cold]
    #[inline(never)]
    fn limit(&self, limit: ParseLimit, max: usize, found: usize, pos: u64) -> WfError {
        WfError::Limit(LimitExceeded::new(limit, max, found, pos + self.bom))
    }

    /// What quick-xml detected, as a well-formedness error at quick-xml's
    /// error position (I/O errors stay quick-xml's).
    #[cold]
    #[inline(never)]
    fn quick_xml_error(&self, e: quick_xml::Error) -> WfError {
        let pos = self.inner.error_position();
        match e {
            quick_xml::Error::Syntax(SyntaxError::InvalidBangMarkup) => self.nwf(
                R::Markup,
                pos,
                "'<!' starts no comment, CDATA section or document type declaration",
            ),
            quick_xml::Error::Syntax(s) => {
                let what = match s {
                    SyntaxError::UnclosedPI => "a processing instruction",
                    SyntaxError::UnclosedXmlDecl => "the XML declaration",
                    SyntaxError::UnclosedComment => "a comment",
                    SyntaxError::UnclosedDoctype => "the document type declaration",
                    SyntaxError::UnclosedCData => "a CDATA section",
                    SyntaxError::UnclosedTag => "a tag",
                    _ => "an attribute value",
                };
                let inside = self.open_elements_shown();
                let message = if inside.is_empty() {
                    format!("the input ends inside {what}")
                } else {
                    format!("the input ends inside {what} in {inside}")
                };
                self.nwf(R::Truncated, pos, message)
            }
            quick_xml::Error::IllFormed(i) => match i {
                IllFormedError::MismatchedEndTag { expected, found } => {
                    let trimmed = found.trim_end_matches(['\u{85}', '\u{2028}', ' ', '\t', '\r', '\n']);
                    if self.version == XmlVersion::V1_1
                        && trimmed == expected
                        && found.contains(['\u{85}', '\u{2028}'])
                    {
                        self.nwf(
                            R::Xml11LineEndInMarkup,
                            pos,
                            format!("the end tag '</{found}>' uses an XML 1.1 line end as white space"),
                        )
                    } else {
                        self.nwf(
                            R::ElementTypeMatch,
                            pos,
                            format!("the end tag '</{found}>' does not match the open element '<{expected}>'"),
                        )
                    }
                }
                IllFormedError::UnmatchedEndTag(name) => self.nwf(
                    R::ElementTypeMatch,
                    pos,
                    format!("the end tag '</{name}>' closes no open element"),
                ),
                IllFormedError::MissingDoctypeName => self.nwf(
                    R::DocumentTypeDeclaration,
                    pos,
                    "a document type declaration without a name",
                ),
                IllFormedError::UnclosedReference => self.nwf(
                    R::Reference,
                    pos,
                    "an '&' that starts no reference ('&' Name ';' or a character reference); a literal '&' is written '&amp;'",
                ),
                IllFormedError::MissingEndTag(_) => self.nwf(
                    R::Truncated,
                    pos,
                    format!("the input ends inside {}", self.open_elements_shown()),
                ),
                IllFormedError::DoubleHyphenInComment => {
                    self.nwf(R::Comment, pos, "'--' inside a comment")
                }
                other => self.nwf(R::XmlDeclaration, pos, other.to_string()),
            },
            // An I/O error, and what `read_event_into` does not raise (the
            // decoding and normalization errors), as a caller's decoding
            // reports them.
            other => decoding_refusal(other, pos + self.bom, self.version),
        }
    }

    /// The innermost open element and how many are open: "the element 'p:b'
    /// (2 elements not closed)"; empty when none is open.
    fn open_elements_shown(&self) -> String {
        match self.open_at.last() {
            None => String::new(),
            Some(&at) => format!(
                "the element '{}' ({} element{} not closed)",
                shown(&self.open[at..]),
                self.open_at.len(),
                if self.open_at.len() == 1 { "" } else { "s" }
            ),
        }
    }

    /// For a caller decoding the event returned last: its start and the
    /// version, to report as this layer's error what its checks rule out
    /// ([`Recheck`]).
    pub(crate) fn recheck(&self) -> Recheck {
        Recheck {
            pos: self.event_start(),
            version: self.version,
        }
    }

    #[cold]
    #[inline(never)]
    fn bad_utf8(&self, pos: u64) -> WfError {
        self.nwf(
            R::Encoding,
            pos,
            "bytes that are not UTF-8 (XML 1.0 §4.3.3: a fatal error; the readers read UTF-8)",
        )
    }

    /// The error for the byte at `c[j]`, which is neither the white space,
    /// the name character nor the delimiter that was expected: bytes that are
    /// not UTF-8, an XML 1.1 line end, or `otherwise`.
    #[cold]
    #[inline(never)]
    fn unexpected(&self, c: &[u8], j: usize, base: u64, otherwise: (R, String)) -> WfError {
        if j < c.len() && c[j] >= 0x80 && decode_at(c, j).is_none() {
            return self.bad_utf8(base + j as u64);
        }
        if self.version == XmlVersion::V1_1 && xml11_line_end_at(c, j).is_some() {
            return self.nwf(
                R::Xml11LineEndInMarkup,
                base + j as u64,
                "a NEL or LINE SEPARATOR used as white space inside markup (XML 1.1 reads it as a line feed; this reader does not support it there)",
            );
        }
        self.nwf(otherwise.0, base + j as u64, otherwise.1)
    }

    #[inline]
    fn count_node(&mut self, pos: u64) -> Result<(), WfError> {
        self.nodes += 1;
        if self.nodes > self.max.nodes {
            return Err(self.limit(ParseLimit::Nodes, self.max.nodes, self.nodes, pos));
        }
        Ok(())
    }

    #[inline]
    fn is_xml_char(&self, v: u32) -> bool {
        if self.version == XmlVersion::V1_1 {
            is_char11(v)
        } else {
            is_char(v)
        }
    }

    /// The first character of `b` that is not allowed raw.
    #[inline]
    fn find_raw_non_char(&self, b: &[u8]) -> Option<usize> {
        if self.version == XmlVersion::V1_1 {
            find_non_char11(b)
        } else {
            find_non_char(b)
        }
    }

    // ── Dispatch ─────────────────────────────────────────────────────────

    #[inline]
    fn check(&mut self, event: &Event<'_>, before: u64, after: u64) -> Result<(), WfError> {
        // Offsets of an event's content: quick-xml hands out what is between
        // the markup delimiters, so the start is the end less the length.
        let len = |b: &[u8]| b.len() as u64;
        match event {
            Event::Text(t) => {
                self.last_start = after - len(t);
                self.check_text(t, self.last_start)
            }
            Event::GeneralRef(r) => {
                self.last_start = after - len(r) - 2;
                self.check_reference_event(r, self.last_start)
            }
            Event::Start(e) => {
                self.run = None;
                self.last_start = after - len(e) - 2;
                self.check_start(e, self.last_start, false)
            }
            Event::Empty(e) => {
                self.run = None;
                self.last_start = after - len(e) - 3;
                self.check_start(e, self.last_start, true)
            }
            Event::End(_) => {
                self.run = None;
                self.last_start = before;
                if let Some(at) = self.open_at.pop() {
                    self.open.truncate(at);
                }
                self.scope.pop(self.depth);
                self.depth = self.depth.saturating_sub(1);
                if self.depth == 0 {
                    self.phase = Phase::Epilog;
                }
                Ok(())
            }
            _ => {
                self.run = None;
                self.check_other(event, before, after)
            }
        }
    }

    /// The events other than text, references and tags.
    fn check_other(&mut self, event: &Event<'_>, before: u64, after: u64) -> Result<(), WfError> {
        let len = |b: &[u8]| b.len() as u64;
        match event {
            Event::CData(c) => {
                self.last_start = after - len(c) - 12;
                self.check_cdata(c, self.last_start)
            }
            Event::Comment(c) => {
                self.last_start = after - len(c) - 7;
                self.check_comment(c, self.last_start)
            }
            Event::PI(p) => {
                self.last_start = after - len(p) - 4;
                self.check_pi(p, self.last_start)
            }
            Event::Decl(d) => {
                let d: &[u8] = d;
                self.last_start = after - len(d) - 4;
                self.check_decl(d, self.last_start)
            }
            Event::DocType(t) => {
                self.last_start = before;
                self.check_doctype(t, before, after)
            }
            Event::Eof => {
                self.last_start = after;
                if self.depth > 0 {
                    return Err(self.nwf(
                        R::Truncated,
                        after,
                        format!("the input ends inside {}", self.open_elements_shown()),
                    ));
                }
                if self.phase == Phase::Prolog {
                    return Err(self.nwf(R::Document, after, "the document has no root element"));
                }
                Ok(())
            }
            Event::Text(_)
            | Event::GeneralRef(_)
            | Event::Start(_)
            | Event::Empty(_)
            | Event::End(_) => {
                unreachable!("handled by `check`")
            }
        }
    }

    // ── Character data ───────────────────────────────────────────────────

    #[inline]
    fn check_text(&mut self, t: &[u8], start: u64) -> Result<(), WfError> {
        if self.depth == 0 {
            return self.check_misc_space(t, start);
        }
        let issue = if self.version == XmlVersion::V1_1 {
            if let Some(i) = find_bad_utf8(t) {
                return Err(self.bad_utf8(start + i as u64));
            }
            find_text_issue11(t)
        } else {
            // One pass: the scan also says whether a byte beyond ASCII came.
            let (issue, high) = find_text_issue(t);
            if high {
                if let Some(i) = find_bad_utf8(t) {
                    return Err(self.bad_utf8(start + i as u64));
                }
            }
            issue
        };
        if let Some((i, rule)) = issue {
            let message = match rule {
                R::CharData => "']]>' in character data".to_string(),
                _ => non_char_message(t, i),
            };
            return Err(self.nwf(rule, start + i as u64, message));
        }
        self.extend_run(start, t.len())
    }

    /// Text outside the root element: white space only (\[27\] `Misc`), and a
    /// UTF-8 BOM at the very start that quick-xml did not remove.
    fn check_misc_space(&mut self, t: &[u8], start: u64) -> Result<(), WfError> {
        let (t, start) = if self.events == 0
            && start == 0
            && self.bom == 0
            && t.starts_with(&[0xEF, 0xBB, 0xBF])
        {
            // A BOM that arrived split across the source's first fills: it is
            // the encoding signature, not text (XML 1.0 §4.3.3), and an XML
            // declaration may follow it.
            self.decl_at = 3;
            if t.len() == 3 {
                self.count_event = false;
                return Ok(());
            }
            (&t[3..], start + 3)
        } else {
            (t, start)
        };
        let mut j = 0;
        while j < t.len() {
            if is_space(t[j]) {
                j += 1;
                continue;
            }
            // An XML 1.1 line end is a line feed there (XML 1.1 §2.11).
            match (self.version, xml11_line_end_at(t, j)) {
                (XmlVersion::V1_1, Some(n)) => j += n,
                _ => {
                    return Err(self.nwf(
                        R::Document,
                        start + j as u64,
                        "character data outside the root element",
                    ))
                }
            }
        }
        if t.len() > self.max.text {
            return Err(self.limit(ParseLimit::TextSize, self.max.text, t.len(), start));
        }
        Ok(())
    }

    #[inline]
    fn extend_run(&mut self, start: u64, bytes: usize) -> Result<(), WfError> {
        let (run_start, size) = match self.run {
            Some((s, n)) => (s, n + bytes),
            None => {
                self.count_node(start)?;
                (start, bytes)
            }
        };
        if size > self.max.text {
            return Err(self.limit(ParseLimit::TextSize, self.max.text, size, run_start));
        }
        self.run = Some((run_start, size));
        Ok(())
    }

    /// A reference in character data (`r` is between `&` and `;`).
    fn check_reference_event(&mut self, r: &[u8], start: u64) -> Result<(), WfError> {
        if self.depth == 0 {
            return Err(self.nwf(R::Document, start, "a reference outside the root element"));
        }
        if let Some(digits) = r.strip_prefix(b"#") {
            self.check_char_ref(digits, start)?;
        } else {
            self.check_entity_name(r, start)?;
        }
        self.extend_run(start, r.len() + 2)
    }

    /// The name of an entity reference at `start` (the `&`): a Name, and one
    /// of the five predefined entities (WFC: Entity Declared — no reader
    /// processes a DTD).
    fn check_entity_name(&self, name: &[u8], start: u64) -> Result<(), WfError> {
        if !is_name(name) {
            if find_bad_utf8(name).is_some() {
                return Err(self.bad_utf8(start + 1));
            }
            return Err(self.nwf(
                R::Reference,
                start,
                format!(
                    "'&{};' is not a reference ([68] EntityRef: '&' Name ';')",
                    shown(name)
                ),
            ));
        }
        if !matches!(name, b"amp" | b"lt" | b"gt" | b"apos" | b"quot") {
            return Err(self.nwf(
                R::EntityDeclared,
                start,
                format!(
                    "the entity '{}' is not declared (only amp, lt, gt, apos and quot are: the readers process no DTD)",
                    shown(name)
                ),
            ));
        }
        Ok(())
    }

    /// The digits of a character reference at `start` (the `&`).
    fn check_char_ref(&self, digits: &[u8], start: u64) -> Result<(), WfError> {
        match char_ref_value(digits) {
            None => Err(self.nwf(
                R::LegalCharacter,
                start,
                format!(
                    "'&#{};' is not a character reference ('&#' digits ';' or '&#x' hex digits ';')",
                    shown(digits)
                ),
            )),
            Some(v) if !self.is_xml_char(v) => Err(self.nwf(
                R::LegalCharacter,
                start,
                format!(
                    "'&#{};' refers to {}, which is not an XML character",
                    shown(digits),
                    code_point(v)
                ),
            )),
            Some(_) => Ok(()),
        }
    }

    fn check_cdata(&mut self, c: &[u8], start: u64) -> Result<(), WfError> {
        if self.depth == 0 {
            return Err(self.nwf(
                R::Document,
                start,
                "a CDATA section outside the root element",
            ));
        }
        if let Some(i) = find_bad_utf8(c) {
            return Err(self.bad_utf8(start + 9 + i as u64));
        }
        if let Some(i) = self.find_raw_non_char(c) {
            return Err(self.nwf(R::Char, start + 9 + i as u64, non_char_message(c, i)));
        }
        self.count_node(start)?;
        if c.len() > self.max.text {
            return Err(self.limit(ParseLimit::TextSize, self.max.text, c.len(), start));
        }
        Ok(())
    }

    fn check_comment(&mut self, c: &[u8], start: u64) -> Result<(), WfError> {
        if let Some(i) = find_bad_utf8(c) {
            return Err(self.bad_utf8(start + 4 + i as u64));
        }
        let issue = if self.version == XmlVersion::V1_1 {
            find_comment_issue11(c)
        } else {
            find_comment_issue(c)
        };
        if let Some((i, rule)) = issue {
            return Err(self.nwf(rule, start + 4 + i as u64, comment_message(c, i, rule)));
        }
        self.count_node(start)?;
        if c.len() > self.max.text {
            return Err(self.limit(ParseLimit::TextSize, self.max.text, c.len(), start));
        }
        Ok(())
    }

    fn check_pi(&mut self, p: &[u8], start: u64) -> Result<(), WfError> {
        let base = start + 2;
        if let Some(i) = find_bad_utf8(p) {
            return Err(self.bad_utf8(base + i as u64));
        }
        self.check_pi_content(p, base)?;
        let target_len = p.iter().position(|&b| is_space(b)).unwrap_or(p.len());
        if target_len > self.max.name {
            return Err(self.limit(ParseLimit::NameLength, self.max.name, target_len, base));
        }
        self.count_node(start)?;
        if p.len() > self.max.text {
            return Err(self.limit(ParseLimit::TextSize, self.max.text, p.len(), start));
        }
        Ok(())
    }

    /// \[16\] `PI`, \[17\] `PITarget` for the content between `<?` and `?>`
    /// (at `base`), as UTF-8.
    fn check_pi_content(&self, p: &[u8], base: u64) -> Result<(), WfError> {
        let Some(target) = scan_name(p, 0) else {
            return Err(self.nwf(
                R::Name,
                base,
                format!(
                    "the processing instruction target '{}' is not a Name",
                    shown(first_token(p))
                ),
            ));
        };
        if target.end < p.len() && !is_space(p[target.end]) {
            return Err(self.unexpected(
                p,
                target.end,
                base,
                (
                    R::ProcessingInstruction,
                    format!(
                        "the processing instruction target '{}' is followed by {}, not by white space",
                        shown(&p[..target.end]),
                        shown_at(p, target.end)
                    ),
                ),
            ));
        }
        let name = &p[..target.end];
        if name.eq_ignore_ascii_case(b"xml") {
            return Err(self.nwf(
                R::ProcessingInstruction,
                base,
                format!(
                    "'{}' is reserved: a processing instruction target is not 'xml' in any case",
                    shown(name)
                ),
            ));
        }
        if target.colons > 0 {
            return Err(self.nwf(
                R::QName,
                base,
                format!(
                    "the processing instruction target '{}' contains a colon (an NCName is required)",
                    shown(name)
                ),
            ));
        }
        if let Some(i) = self.find_raw_non_char(p) {
            return Err(self.nwf(R::Char, base + i as u64, non_char_message(p, i)));
        }
        Ok(())
    }

    // ── XML and document type declarations ───────────────────────────────

    /// `d` is the declaration between `<?` and `?>`, starting with `xml`.
    fn check_decl(&mut self, d: &[u8], start: u64) -> Result<(), WfError> {
        if self.events > 0 || start != self.decl_at {
            return Err(self.nwf(
                R::XmlDeclaration,
                start,
                "an XML declaration that is not at the start of the document",
            ));
        }
        let base = start + 2;
        let bad = |i: usize, m: &str| {
            let m = if d.windows(2).any(|w| w == [0xC2, 0x85])
                || d.windows(3).any(|w| w == [0xE2, 0x80, 0xA8])
            {
                "a NEL or LINE SEPARATOR inside the XML declaration (XML 1.1 §2.11: a fatal error)"
            } else {
                m
            };
            self.nwf(
                R::XmlDeclaration,
                base + i as u64,
                format!(
                    "{m} (the declaration is '<?xml' S version, then optionally S encoding, then optionally S standalone 'yes' or 'no')"
                ),
            )
        };
        let i = 3;
        let s = skip_space(d, i);
        if s == i || !d[s..].starts_with(b"version") {
            return Err(bad(s, "the XML declaration does not start with version"));
        }
        let (version, mut i) =
            pseudo_value(d, s + 7).ok_or_else(|| bad(s, "version has no quoted value"))?;
        let grammatical = version.len() > 2
            && version.starts_with(b"1.")
            && version[2..].iter().all(u8::is_ascii_digit);
        if !grammatical {
            return Err(bad(
                s,
                &format!(
                    "version '{}' is not a VersionNum ('1.' digits)",
                    shown(version)
                ),
            ));
        }
        let mut encoding = None;
        let s = skip_space(d, i);
        if s > i && d[s..].starts_with(b"encoding") {
            let (enc, j) =
                pseudo_value(d, s + 8).ok_or_else(|| bad(s, "encoding has no quoted value"))?;
            let grammatical = enc.first().is_some_and(u8::is_ascii_alphabetic)
                && enc
                    .iter()
                    .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
            if !grammatical {
                return Err(bad(
                    s,
                    &format!("encoding '{}' is not an EncName", shown(enc)),
                ));
            }
            encoding = Some(enc);
            i = j;
        }
        let mut standalone = None;
        let s = skip_space(d, i);
        if s > i && d[s..].starts_with(b"standalone") {
            let (sd, j) =
                pseudo_value(d, s + 10).ok_or_else(|| bad(s, "standalone has no quoted value"))?;
            standalone = match sd {
                b"yes" => Some(true),
                b"no" => Some(false),
                _ => {
                    return Err(bad(
                        s,
                        &format!("standalone is '{}', not 'yes' or 'no'", shown(sd)),
                    ))
                }
            };
            i = j;
        }
        let s = skip_space(d, i);
        if s != d.len() {
            return Err(bad(
                s,
                &format!("unexpected {} in the XML declaration", shown_at(d, s)),
            ));
        }
        // The grammar is ASCII, so these are UTF-8.
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        let declaration = XmlDeclaration {
            version: text(version),
            encoding: encoding.map(text),
            standalone,
        };
        self.version = declaration.xml_version();
        self.declaration = Some(declaration);
        Ok(())
    }

    /// `t` is the declaration after `<!DOCTYPE` and white space, up to `>`.
    fn check_doctype(&mut self, t: &[u8], before: u64, after: u64) -> Result<(), WfError> {
        let name_pos = after - 1 - t.len() as u64;
        let trimming = self.inner.config().trim_text_start;
        // With white space skipped before markup, `before` is where that
        // white space starts; the name's offset is exact either way.
        let pos = if trimming { name_pos } else { before };
        if self.phase != Phase::Prolog {
            return Err(self.nwf(
                R::DocumentTypeDeclaration,
                pos,
                "a document type declaration after the start of the root element (it belongs in the prolog)",
            ));
        }
        if self.seen_doctype {
            return Err(self.nwf(
                R::DocumentTypeDeclaration,
                pos,
                "a second document type declaration",
            ));
        }
        self.seen_doctype = true;
        if !trimming && after - before == t.len() as u64 + 10 {
            // `<!DOCTYPE` + content + `>`: no white space after the keyword
            // (an XML 1.1 line end there is named as such).
            return Err(self.unexpected(
                t,
                0,
                before + 9,
                (
                    R::DocumentTypeDeclaration,
                    "'<!DOCTYPE' is not followed by white space".into(),
                ),
            ));
        }
        if t.len() > self.max.text {
            return Err(self.limit(ParseLimit::TextSize, self.max.text, t.len(), name_pos));
        }
        if let Some(i) = find_bad_utf8(t) {
            return Err(self.bad_utf8(name_pos + i as u64));
        }
        if let Some(i) = self.find_raw_non_char(t) {
            return Err(self.nwf(R::Char, name_pos + i as u64, non_char_message(t, i)));
        }
        self.check_doctypedecl(t, name_pos)?;
        self.doctype_pending = Some(PendingDoctype {
            after,
            len: t.len(),
            hash: fnv1a(t),
        });
        Ok(())
    }

    /// \[28\] `doctypedecl` after `'<!DOCTYPE' S`: `Name (S ExternalID)? S?
    /// ('[' intSubset ']' S?)?` — the internal subset read for balance
    /// (literals, comments, PIs, markup declarations, parameter-entity
    /// references), its declarations not processed.
    fn check_doctypedecl(&self, t: &[u8], base: u64) -> Result<(), WfError> {
        let dtd = |i: usize, m: String| self.nwf(R::DocumentTypeDeclaration, base + i as u64, m);
        let Some(name) = scan_name(t, 0) else {
            return Err(self.nwf(
                R::Name,
                base,
                format!(
                    "the document type name '{}' is not a Name",
                    shown(first_token(t))
                ),
            ));
        };
        let mut i = name.end;
        if i < t.len() && !is_space(t[i]) && t[i] != b'[' {
            return Err(self.unexpected(
                t,
                i,
                base,
                (
                    R::DocumentTypeDeclaration,
                    format!("the document type name is followed by {}", shown_at(t, i)),
                ),
            ));
        }
        let s = skip_space(t, i);
        if s > i && (t[s..].starts_with(b"SYSTEM") || t[s..].starts_with(b"PUBLIC")) {
            let public = t[s..].starts_with(b"PUBLIC");
            let mut j = s + 6;
            let k = skip_space(t, j);
            if k == j {
                return Err(self.unexpected(
                    t,
                    j,
                    base,
                    (
                        R::DocumentTypeDeclaration,
                        format!(
                            "{} is not followed by white space and a literal",
                            if public { "PUBLIC" } else { "SYSTEM" }
                        ),
                    ),
                ));
            }
            j = k;
            if public {
                let Some((lit, end)) = quoted_literal(t, j) else {
                    return Err(dtd(
                        j,
                        "PUBLIC is not followed by a quoted public identifier ([12] PubidLiteral)"
                            .into(),
                    ));
                };
                if let Some(p) = lit.iter().position(|&c| !is_pubid_char(c)) {
                    return Err(dtd(
                        j + 1 + p,
                        format!(
                            "{} is not allowed in a public identifier ([13] PubidChar)",
                            shown_at(lit, p)
                        ),
                    ));
                }
                let k = skip_space(t, end);
                if k == end {
                    return Err(self.unexpected(
                        t,
                        end,
                        base,
                        (
                            R::DocumentTypeDeclaration,
                            "the public identifier is not followed by white space and a system literal"
                                .into(),
                        ),
                    ));
                }
                j = k;
            }
            let Some((_, end)) = quoted_literal(t, j) else {
                return Err(dtd(
                    j,
                    "a system literal is not quoted ([11] SystemLiteral)".into(),
                ));
            };
            i = end;
        } else if s < t.len() && t[s] != b'[' {
            return Err(self.unexpected(
                t,
                s,
                base,
                (
                    R::DocumentTypeDeclaration,
                    format!(
                        "{} where an external identifier or '[' was expected",
                        shown_at(t, s)
                    ),
                ),
            ));
        }
        let s = skip_space(t, i);
        if s < t.len() && t[s] == b'[' {
            i = self.check_internal_subset(t, s + 1, base)?;
        } else if s < t.len() {
            return Err(self.unexpected(
                t,
                s,
                base,
                (
                    R::DocumentTypeDeclaration,
                    format!("{} where '[' or '>' was expected", shown_at(t, s)),
                ),
            ));
        }
        let s = skip_space(t, i);
        if s != t.len() {
            return Err(self.unexpected(
                t,
                s,
                base,
                (
                    R::DocumentTypeDeclaration,
                    format!("{} after the internal subset", shown_at(t, s)),
                ),
            ));
        }
        Ok(())
    }

    /// The internal subset from `t[i]` (after `[`): the offset after its `]`.
    fn check_internal_subset(&self, t: &[u8], mut i: usize, base: u64) -> Result<usize, WfError> {
        let dtd = |i: usize, m: String| self.nwf(R::DocumentTypeDeclaration, base + i as u64, m);
        loop {
            i = skip_space(t, i);
            let rest = &t[i.min(t.len())..];
            if rest.is_empty() {
                return Err(dtd(i, "the internal subset is not closed by ']'".into()));
            }
            if rest[0] == b']' {
                return Ok(i + 1);
            }
            if rest[0] == b'%' {
                // [69] PEReference ::= '%' Name ';'
                match scan_name(t, i + 1) {
                    Some(n) if t.get(n.end) == Some(&b';') => i = n.end + 1,
                    _ => {
                        return Err(dtd(
                            i,
                            "'%' does not start a parameter-entity reference ('%' Name ';')".into(),
                        ))
                    }
                }
                continue;
            }
            if rest.starts_with(b"<!--") {
                let Some(end) = find(t, i + 4, b"-->") else {
                    return Err(dtd(
                        i,
                        "a comment in the internal subset is not closed".into(),
                    ));
                };
                let body = &t[i + 4..end];
                if let Some(p) = find(body, 0, b"--").or(if body.ends_with(b"-") {
                    Some(body.len() - 1)
                } else {
                    None
                }) {
                    return Err(self.nwf(
                        R::Comment,
                        base + (i + 4 + p) as u64,
                        comment_message(body, p, R::Comment),
                    ));
                }
                i = end + 3;
                continue;
            }
            if rest.starts_with(b"<?") {
                let Some(end) = find(t, i + 2, b"?>") else {
                    return Err(dtd(
                        i,
                        "a processing instruction in the internal subset is not closed".into(),
                    ));
                };
                self.check_pi_content(&t[i + 2..end], base + (i + 2) as u64)?;
                i = end + 2;
                continue;
            }
            let decl = [&b"<!ELEMENT"[..], b"<!ATTLIST", b"<!ENTITY", b"<!NOTATION"]
                .into_iter()
                .find(|k| rest.starts_with(k) && rest.get(k.len()).copied().is_some_and(is_space));
            let Some(keyword) = decl else {
                let what = (
                    R::DocumentTypeDeclaration,
                    format!(
                        "'{}' is not a markup declaration, comment, processing instruction or parameter-entity reference",
                        shown(first_token(rest))
                    ),
                );
                // A keyword followed by something other than white space: an
                // XML 1.1 line end there is named as such.
                if let Some(k) = [&b"<!ELEMENT"[..], b"<!ATTLIST", b"<!ENTITY", b"<!NOTATION"]
                    .into_iter()
                    .find(|k| rest.starts_with(k))
                {
                    let j = i + k.len();
                    if self.version == XmlVersion::V1_1 && xml11_line_end_at(t, j).is_some() {
                        return Err(self.unexpected(t, j, base, what));
                    }
                }
                return Err(self.nwf(what.0, base + i as u64, what.1));
            };
            // To the `>` that ends the declaration, past quoted literals.
            let mut j = i + keyword.len();
            loop {
                match t.get(j) {
                    None => return Err(dtd(i, "a markup declaration is not closed by '>'".into())),
                    Some(b'>') => break,
                    Some(&q @ (b'"' | b'\'')) => match t[j + 1..].iter().position(|&c| c == q) {
                        Some(p) => j += p + 2,
                        None => return Err(dtd(j, "a literal is not closed".into())),
                    },
                    Some(b'<') => return Err(dtd(j, "'<' inside a markup declaration".into())),
                    Some(_) => j += 1,
                }
            }
            i = j + 1;
        }
    }

    /// The markup of the declaration in `p`, if `buf` holds it.
    fn check_doctype_markup(&self, buf: &[u8], p: PendingDoctype) -> Result<(), WfError> {
        // Only the buffer the declaration was read into is looked at: one
        // that does not end in its content and `>` is another caller's.
        let fits = buf.len() >= p.len + 10
            && buf.last() == Some(&b'>')
            && fnv1a(&buf[buf.len() - 1 - p.len..buf.len() - 1]) == p.hash;
        if !fits {
            return Ok(());
        }
        let start = p.after.saturating_sub(buf.len() as u64);
        if !buf.starts_with(b"<!DOCTYPE") {
            return Err(self.nwf(
                R::DocumentTypeDeclaration,
                start,
                format!(
                    "'{}' is not '<!DOCTYPE' (the keyword is upper case)",
                    shown(&buf[..9])
                ),
            ));
        }
        if !is_space(buf[9]) {
            return Err(self.unexpected(
                buf,
                9,
                start,
                (
                    R::DocumentTypeDeclaration,
                    "'<!DOCTYPE' is not followed by white space".into(),
                ),
            ));
        }
        Ok(())
    }

    // ── Start tags ───────────────────────────────────────────────────────

    /// `c` is the tag between `<` and `>` (or `/>`); `start` is its `<`.
    fn check_start(&mut self, c: &[u8], start: u64, is_empty: bool) -> Result<(), WfError> {
        let base = start + 1;
        if self.depth == 0 && self.phase == Phase::Epilog {
            return Err(self.nwf(
                R::Document,
                start,
                "a second element after the root element",
            ));
        }
        let Some(name) = scan_name(c, 0) else {
            return Err(self.unexpected(
                c,
                0,
                base,
                (
                    R::Name,
                    format!(
                        "'<' is followed by {}, not by an element name; a literal '<' is written '&lt;'",
                        shown_at(c, 0)
                    ),
                ),
            ));
        };
        if name.end < c.len() && !is_space(c[name.end]) {
            return Err(self.unexpected(
                c,
                name.end,
                base,
                (
                    R::StartTag,
                    format!(
                        "the element name '{}' is followed by {}, not by white space, '>' or '/>'",
                        shown(&c[..name.end]),
                        shown_at(c, name.end)
                    ),
                ),
            ));
        }
        self.check_qname(c, 0, &name, "element", base)?;
        if name.colons == 1 && &c[..name.first_colon.unwrap_or(0)] == b"xmlns" {
            return Err(self.nwf(
                R::ReservedPrefix,
                base,
                "an element name with the prefix 'xmlns'",
            ));
        }
        if name.end > self.max.name {
            return Err(self.limit(ParseLimit::NameLength, self.max.name, name.end, base));
        }
        if self.depth + 1 > self.max.depth {
            return Err(self.limit(ParseLimit::Depth, self.max.depth, self.depth + 1, start));
        }
        self.count_node(start)?;

        self.attrs.clear();
        let mut attributes = 0usize;
        let mut declarations = 0usize;
        let mut prefixed = 0usize;
        let mut i = name.end;
        loop {
            let s = i;
            i = skip_space(c, i);
            if i >= c.len() {
                break;
            }
            if i == s {
                return Err(self.unexpected(
                    c,
                    i,
                    base,
                    (
                        R::StartTag,
                        format!(
                            "{} is not preceded by white space ([40] STag: (S Attribute)*)",
                            shown_at(c, i)
                        ),
                    ),
                ));
            }
            let key_start = i;
            let Some(key) = scan_name(c, i) else {
                // A name character that cannot start a name is a bad name;
                // anything else is a stray character in the tag.
                let rule = match decode_at_or_ascii(c, i) {
                    Some(cp) if is_name_char(cp) => R::Name,
                    _ => R::StartTag,
                };
                return Err(self.unexpected(
                    c,
                    i,
                    base,
                    (
                        rule,
                        format!("{} does not start an attribute name", shown_at(c, i)),
                    ),
                ));
            };
            i = skip_space(c, key.end);
            if c.get(i) != Some(&b'=') {
                return Err(self.unexpected(
                    c,
                    i,
                    base,
                    (
                        R::StartTag,
                        format!(
                            "the attribute name '{}' is followed by {}, not by '='",
                            shown(&c[key_start..key.end]),
                            shown_at(c, i)
                        ),
                    ),
                ));
            }
            i = skip_space(c, i + 1);
            let quote = match c.get(i) {
                Some(&q @ (b'"' | b'\'')) => q,
                _ => {
                    return Err(self.unexpected(
                        c,
                        i,
                        base,
                        (
                            R::StartTag,
                            format!(
                                "the value of attribute '{}' is not quoted",
                                shown(&c[key_start..key.end])
                            ),
                        ),
                    ))
                }
            };
            let value_start = i + 1;
            let (value_end, needs_normalization) = if self.version == XmlVersion::V1_1 {
                self.scan_value11(c, value_start, quote, base)?
            } else {
                self.scan_value(c, value_start, quote, base)?
            };
            i = value_end + 1;
            self.check_qname(c, key_start, &key, "attribute", base)?;
            let key_pos = base + key_start as u64;
            let qname = &c[key_start..key.end];
            let value_len = value_end - value_start;
            if value_len > self.max.value {
                return Err(self.limit(
                    ParseLimit::AttributeValueSize,
                    self.max.value,
                    value_len,
                    key_pos,
                ));
            }
            let kind = if key.colons == 0 && qname == b"xmlns" {
                AttrKind::DefaultNamespace
            } else if key.first_colon == Some(key_start + 5) && qname.starts_with(b"xmlns") {
                AttrKind::PrefixNamespace
            } else {
                AttrKind::Attribute
            };
            let span = AttrSpan {
                key: (key_start, key.end),
                colon: key.first_colon,
                value: (value_start, value_end),
                kind,
                needs_normalization,
            };
            if kind == AttrKind::Attribute {
                attributes += 1;
                if attributes > self.max.attributes {
                    return Err(self.limit(
                        ParseLimit::Attributes,
                        self.max.attributes,
                        attributes,
                        key_pos,
                    ));
                }
                if qname.len() > self.max.name {
                    return Err(self.limit(
                        ParseLimit::NameLength,
                        self.max.name,
                        qname.len(),
                        key_pos,
                    ));
                }
                if key.first_colon.is_some() {
                    prefixed += 1;
                }
            } else {
                declarations += 1;
                if declarations > self.max.declarations {
                    return Err(self.limit(
                        ParseLimit::NamespaceDeclarations,
                        self.max.declarations,
                        declarations,
                        key_pos,
                    ));
                }
                let prefix_len = span.prefix(c).len();
                if prefix_len > self.max.name {
                    return Err(self.limit(
                        ParseLimit::NameLength,
                        self.max.name,
                        prefix_len,
                        key_pos,
                    ));
                }
                if value_len > self.max.uri {
                    return Err(self.limit(
                        ParseLimit::NamespaceUriLength,
                        self.max.uri,
                        value_len,
                        key_pos,
                    ));
                }
                self.check_declaration(c, &span, key_pos)?;
            }
            self.count_node(key_pos)?;
            self.attrs.push(span);
        }

        if self.attrs.len() >= 2 {
            self.check_unique_att_spec(c, base)?;
        }
        let depth = self.depth + 1;
        if declarations > 0 {
            self.push_declarations(c, depth, start)?;
        }
        if name.colons == 1 || prefixed > 0 {
            self.check_prefixes_declared(c, &name, base)?;
        }
        if prefixed >= 2 {
            self.check_attributes_unique(c, base)?;
        }
        if is_empty {
            self.scope.pop(depth);
            if self.depth == 0 {
                self.phase = Phase::Epilog;
            }
        } else {
            self.depth = depth;
            self.phase = Phase::Root;
            self.open_at.push(self.open.len());
            self.open.extend_from_slice(&c[..name.end]);
        }
        Ok(())
    }

    /// Namespaces in XML 1.0 §4 \[7\] `QName`: no colon, or one between two
    /// non-empty parts (each then an `NCName`, the name being a `Name`).
    fn check_qname(
        &self,
        c: &[u8],
        start: usize,
        name: &NameScan,
        what: &str,
        base: u64,
    ) -> Result<(), WfError> {
        // The prefix starts where the name does, with a NameStartChar; the
        // local part must start with one too ([4] NCName).
        let ok = match name.first_colon {
            None => true,
            Some(colon) => {
                name.colons == 1
                    && colon > start
                    && colon + 1 < name.end
                    && decode_at_or_ascii(c, colon + 1).is_some_and(is_name_start)
            }
        };
        if ok {
            return Ok(());
        }
        Err(self.nwf(
            R::QName,
            base + start as u64,
            format!(
                "the {what} name '{}' is not a qualified name (Prefix ':' LocalPart, each an NCName)",
                shown(&c[start..name.end])
            ),
        ))
    }

    /// An attribute value from `c[start]` to the closing `quote`: the offset
    /// of that quote, and whether the value needs normalization (it holds a
    /// reference, tab, line feed or carriage return). Checks `<`, every
    /// reference, \[2\] `Char` and UTF-8.
    #[inline]
    fn scan_value(
        &self,
        c: &[u8],
        start: usize,
        quote: u8,
        base: u64,
    ) -> Result<(usize, bool), WfError> {
        let mut j = start;
        let mut normalize = false;
        loop {
            while j + 16 <= c.len() {
                let mut flag = false;
                for &b in &c[j..j + 16] {
                    // `b < 0x20 || b >= 0x80` in one comparison.
                    flag |=
                        (b == quote) | (b == b'<') | (b == b'&') | (b.wrapping_sub(0x20) >= 0x60);
                }
                if flag {
                    break;
                }
                j += 16;
            }
            let Some(&b) = c.get(j) else {
                return Err(self.nwf(
                    R::StartTag,
                    base + start as u64 - 1,
                    "an attribute value is not terminated",
                ));
            };
            match b {
                _ if b == quote => return Ok((j, normalize)),
                b'<' => {
                    return Err(self.nwf(
                        R::AttributeValue,
                        base + j as u64,
                        "'<' in an attribute value (it is written '&lt;')",
                    ))
                }
                b'&' => {
                    normalize = true;
                    j = self.check_value_reference(c, j, quote, base)?;
                }
                b'\t' | b'\n' | b'\r' => {
                    normalize = true;
                    j += 1
                }
                0..=0x1F => return Err(self.nwf(R::Char, base + j as u64, non_char_message(c, j))),
                0x80.. => match decode_at(c, j) {
                    Some((0xFFFE | 0xFFFF, _)) => {
                        return Err(self.nwf(R::Char, base + j as u64, non_char_message(c, j)))
                    }
                    Some((_, n)) => j += n,
                    None => return Err(self.bad_utf8(base + j as u64)),
                },
                _ => j += 1,
            }
        }
    }

    /// [`scan_value`](Self::scan_value) with the XML 1.1 rules: the
    /// restricted characters (\[2a\]) are refused raw, a NEL or LINE
    /// SEPARATOR is a line end (normalized to a space).
    #[cold]
    #[inline(never)]
    fn scan_value11(
        &self,
        c: &[u8],
        start: usize,
        quote: u8,
        base: u64,
    ) -> Result<(usize, bool), WfError> {
        let mut j = start;
        let mut normalize = false;
        loop {
            let Some(&b) = c.get(j) else {
                return Err(self.nwf(
                    R::StartTag,
                    base + start as u64 - 1,
                    "an attribute value is not terminated",
                ));
            };
            match b {
                _ if b == quote => return Ok((j, normalize)),
                b'<' => {
                    return Err(self.nwf(
                        R::AttributeValue,
                        base + j as u64,
                        "'<' in an attribute value (it is written '&lt;')",
                    ))
                }
                b'&' => {
                    normalize = true;
                    j = self.check_value_reference(c, j, quote, base)?;
                }
                b'\t' | b'\n' | b'\r' => {
                    normalize = true;
                    j += 1
                }
                0..=0x1F | 0x7F => {
                    return Err(self.nwf(R::Char, base + j as u64, non_char_message(c, j)))
                }
                0x80.. => match decode_at(c, j) {
                    Some((0x85 | 0x2028, n)) => {
                        normalize = true;
                        j += n
                    }
                    Some((0x80..=0x9F | 0xFFFE | 0xFFFF, _)) => {
                        return Err(self.nwf(R::Char, base + j as u64, non_char_message(c, j)))
                    }
                    Some((_, n)) => j += n,
                    None => return Err(self.bad_utf8(base + j as u64)),
                },
                _ => j += 1,
            }
        }
    }

    /// The reference at `c[at] == '&'` inside an attribute value; the offset
    /// after its `;`.
    fn check_value_reference(
        &self,
        c: &[u8],
        at: usize,
        quote: u8,
        base: u64,
    ) -> Result<usize, WfError> {
        let pos = base + at as u64;
        if c.get(at + 1) == Some(&b'#') {
            let mut k = at + 2;
            while k < c.len() && c[k].is_ascii_alphanumeric() {
                k += 1;
            }
            if c.get(k) != Some(&b';') {
                return Err(self.nwf(
                    R::LegalCharacter,
                    pos,
                    format!(
                        "'{}' is not a character reference ('&#' digits ';' or '&#x' hex digits ';')",
                        shown(&c[at..k.min(c.len())])
                    ),
                ));
            }
            self.check_char_ref(&c[at + 2..k], pos)?;
            return Ok(k + 1);
        }
        match scan_name(c, at + 1) {
            Some(n) if c.get(n.end) == Some(&b';') => {
                self.check_entity_name(&c[at + 1..n.end], pos)?;
                Ok(n.end + 1)
            }
            _ => {
                let end = c[at..]
                    .iter()
                    .position(|&b| b == quote || b == b';' || is_space(b))
                    .map_or(c.len(), |p| at + p);
                if find_bad_utf8(&c[at..end]).is_some() {
                    return Err(self.bad_utf8(pos + 1));
                }
                Err(self.nwf(
                    R::Reference,
                    pos,
                    format!(
                        "'{}' does not start a reference ('&' Name ';' or a character reference); a literal '&' is written '&amp;'",
                        shown(&c[at..end])
                    ),
                ))
            }
        }
    }

    /// The reserved prefixes and namespace names, and no prefix undeclaring
    /// (Namespaces in XML 1.0); in XML 1.1 a prefix other than `xml` and
    /// `xmlns` may be undeclared.
    fn check_declaration(&self, c: &[u8], d: &AttrSpan, pos: u64) -> Result<(), WfError> {
        let prefix = d.prefix(c);
        let value = self.namespace_name(c, d, pos)?;
        let value = value.as_bytes();
        let is_default = prefix.is_empty();
        let xml_ns = XML_NAMESPACE.as_bytes();
        let xmlns_ns = XMLNS_NAMESPACE.as_bytes();
        let fail = |m: String| Err(self.nwf(R::ReservedPrefix, pos, m));
        if !is_default && prefix == b"xmlns" {
            return fail("the prefix 'xmlns' is declared (it is bound by definition and must not be declared or undeclared)".into());
        }
        if !is_default && prefix == b"xml" {
            if value != xml_ns {
                return fail(format!(
                    "the prefix 'xml' is bound to '{}' (it may only be bound to {XML_NAMESPACE}, and not undeclared)",
                    shown(value)
                ));
            }
            return Ok(());
        }
        if value == xml_ns {
            return fail(format!(
                "{} is bound to the XML namespace {XML_NAMESPACE}, which only the prefix 'xml' is",
                if is_default {
                    "the default namespace".to_string()
                } else {
                    format!("the prefix '{}'", shown(prefix))
                }
            ));
        }
        if value == xmlns_ns {
            return fail(format!(
                "{} is bound to the xmlns namespace {XMLNS_NAMESPACE}, which no declaration may bind",
                if is_default {
                    "the default namespace".to_string()
                } else {
                    format!("the prefix '{}'", shown(prefix))
                }
            ));
        }
        if !is_default && value.is_empty() && self.version == XmlVersion::V1_0 {
            return Err(self.nwf(
                R::PrefixUndeclaring,
                pos,
                format!(
                    "xmlns:{}=\"\" undeclares a prefix (only the default namespace may be undeclared in XML 1.0)",
                    shown(prefix)
                ),
            ));
        }
        Ok(())
    }

    /// The namespace name a declaration binds: its value normalized as the
    /// readers hand attribute values to their callers (`AttrSpan::value`,
    /// with the document's version: XML 1.0 / 1.1 §3.3.3 with §2.11 line
    /// ends), so the scope, the reserved-name checks and NSC: Attributes
    /// Unique compare what the callers see. `pos` is the declaration's.
    fn namespace_name<'t>(
        &self,
        c: &'t [u8],
        d: &AttrSpan,
        pos: u64,
    ) -> Result<Cow<'t, str>, WfError> {
        d.value(c, self.quick_xml_version())
            .map_err(|e| decoding_refusal(e, pos + self.bom, self.version))
    }

    /// Brings this tag's declarations into scope, checking the
    /// namespaces-in-scope limit on the prefixes bound once they all apply
    /// (an undeclaration removes one). Linear in the declarations: refused
    /// as soon as the count, less the undeclarations still to come, is over
    /// the limit (`found` is then that lower bound of the element's count).
    fn push_declarations(&mut self, c: &[u8], depth: usize, start: u64) -> Result<(), WfError> {
        let limited = self.max.in_scope != usize::MAX;
        // Undeclarations (an empty value: normalization never empties one)
        // still to come on this tag.
        let mut undeclaring = if limited {
            self.attrs
                .iter()
                .filter(|d| d.kind != AttrKind::Attribute && d.value.0 == d.value.1)
                .count()
        } else {
            0
        };
        for n in 0..self.attrs.len() {
            let d = self.attrs[n];
            if d.kind == AttrKind::Attribute {
                continue;
            }
            let value = self.namespace_name(c, &d, start + 1 + d.key.0 as u64)?;
            self.scope.push(depth, d.prefix(c), value.as_bytes());
            if limited {
                if d.value.0 == d.value.1 {
                    undeclaring -= 1;
                }
                let at_least = self.scope.in_scope().saturating_sub(undeclaring);
                if at_least > self.max.in_scope {
                    return Err(self.limit(
                        ParseLimit::NamespacesInScope,
                        self.max.in_scope,
                        at_least,
                        start,
                    ));
                }
            }
        }
        Ok(())
    }

    /// NSC: Prefix Declared — the element's prefix and every attribute's,
    /// against the scope with this tag's declarations in it.
    fn check_prefixes_declared(&self, c: &[u8], name: &NameScan, base: u64) -> Result<(), WfError> {
        let unbound = |prefix: &[u8], pos: u64, what: &str| {
            Err(self.nwf(
                R::PrefixDeclared,
                pos,
                format!(
                    "the prefix '{}' of the {what} name is not bound to a namespace in scope",
                    shown(prefix)
                ),
            ))
        };
        if let Some(colon) = name.first_colon {
            let prefix = &c[..colon];
            if self.scope.resolve(prefix).is_none() {
                return unbound(prefix, base, "element");
            }
        }
        for a in &self.attrs {
            if a.kind != AttrKind::Attribute || a.colon.is_none() {
                continue;
            }
            let prefix = a.prefix(c);
            if self.scope.resolve(prefix).is_none() {
                return unbound(prefix, base + a.key.0 as u64, "attribute");
            }
        }
        Ok(())
    }

    /// XML 1.0 §3.1 WFC: Unique Att Spec — no attribute name twice in one
    /// tag (namespace declarations included).
    fn check_unique_att_spec(&self, c: &[u8], base: u64) -> Result<(), WfError> {
        let dup = |at: &AttrSpan| {
            Err(self.nwf(
                R::UniqueAttSpec,
                base + at.key.0 as u64,
                format!(
                    "the attribute '{}' appears twice in one tag",
                    shown(at.key(c))
                ),
            ))
        };
        let attrs = &self.attrs;
        if attrs.len() <= 16 {
            for (i, at) in attrs.iter().enumerate().skip(1) {
                if attrs[..i].iter().any(|p| p.key(c) == at.key(c)) {
                    return dup(at);
                }
            }
            return Ok(());
        }
        let mut seen: FastMap<&[u8], ()> = FastMap::default();
        seen.reserve(attrs.len());
        for at in attrs {
            if seen.insert(at.key(c), ()).is_some() {
                return dup(at);
            }
        }
        Ok(())
    }

    /// Namespaces in XML 1.0 §6.3: no two attributes with the same expanded
    /// name (every prefix is bound by now; identical qualified names were
    /// refused as duplicates).
    fn check_attributes_unique(&self, c: &[u8], base: u64) -> Result<(), WfError> {
        let resolve = |p: &AttrSpan| self.scope.resolve(p.prefix(c));
        let fail = |first: &AttrSpan, second: &AttrSpan| {
            Err(self.nwf(
                R::AttributesUnique,
                base + second.key.0 as u64,
                format!(
                    "the attributes '{}' and '{}' have the same expanded name",
                    shown(first.key(c)),
                    shown(second.key(c))
                ),
            ))
        };
        let prefixed: Vec<&AttrSpan> = self
            .attrs
            .iter()
            .filter(|a| a.kind == AttrKind::Attribute && a.colon.is_some())
            .collect();
        if prefixed.len() <= 8 {
            for (i, a) in prefixed.iter().enumerate() {
                for b in &prefixed[i + 1..] {
                    if a.local(c) != b.local(c) {
                        continue;
                    }
                    if let (Some(x), Some(y)) = (resolve(a), resolve(b)) {
                        if x.0 == y.0 && x.1 == y.1 {
                            return fail(a, b);
                        }
                    }
                }
            }
            return Ok(());
        }
        // Keyed by the expanded name itself, so the work stays linear
        // whatever the names hash to.
        let mut seen: FastMap<(&[u8], &[u8]), &AttrSpan> = FastMap::default();
        seen.reserve(prefixed.len());
        for b in prefixed {
            let Some((_, uri)) = resolve(b) else {
                continue;
            };
            if let Some(a) = seen.insert((b.local(c), uri), b) {
                return fail(a, b);
            }
        }
        Ok(())
    }
}

// ── Scanning helpers ─────────────────────────────────────────────────────

/// The code point at `b[i]`, if it is well-formed UTF-8.
fn decode_at_or_ascii(b: &[u8], i: usize) -> Option<u32> {
    match b.get(i) {
        Some(&c) if c < 0x80 => Some(u32::from(c)),
        Some(_) => decode_at(b, i).map(|(cp, _)| cp),
        None => None,
    }
}

fn skip_space(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    i
}

/// The first `needle` in `hay` at or after `from`.
fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// A quoted literal at `t[j]` (\[11\] `SystemLiteral`, \[12\]
/// `PubidLiteral`): its content and the offset after the closing quote.
fn quoted_literal(t: &[u8], j: usize) -> Option<(&[u8], usize)> {
    let q = *t.get(j)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let len = t[j + 1..].iter().position(|&c| c == q)?;
    Some((&t[j + 1..j + 1 + len], j + len + 2))
}

/// \[13\] `PubidChar ::= #x20 | #xD | #xA | [a-zA-Z0-9] | [-'()+,./:=?;!*#@$_%]`
/// (the quote that delimits the literal never reaches here).
fn is_pubid_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b" \r\n-'()+,./:=?;!*#@$_%".contains(&c)
}

/// `S? '=' S?` then a quoted value at `i` (\[25\] `Eq`): the value and the
/// offset after its closing quote.
fn pseudo_value(b: &[u8], i: usize) -> Option<(&[u8], usize)> {
    let i = skip_space(b, i);
    if b.get(i) != Some(&b'=') {
        return None;
    }
    let i = skip_space(b, i + 1);
    let q = *b.get(i)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let len = b[i + 1..].iter().position(|&c| c == q)?;
    Some((&b[i + 1..i + 1 + len], i + len + 2))
}

/// The first character of a text run that is not a \[2\] `Char`, or the first
/// `]]>` (\[14\] `CharData`), in one pass — and whether the run has a byte
/// beyond ASCII (then its UTF-8 is checked).
#[inline]
fn find_text_issue(b: &[u8]) -> (Option<(usize, R)>, bool) {
    let mut i = 0;
    let mut high = 0u8;
    while i < b.len() {
        let end = (i + 32).min(b.len());
        let mut flag = false;
        for &c in &b[i..end] {
            flag |= ((c < 0x20) & (c != b'\t') & (c != b'\n') & (c != b'\r'))
                | (c == 0xEF)
                | (c == b']');
            high |= c;
        }
        if flag {
            for j in i..end {
                match b[j] {
                    b']' if b[j..].starts_with(b"]]>") => return (Some((j, R::CharData)), true),
                    b'\t' | b'\n' | b'\r' => {}
                    0..=0x1F => return (Some((j, R::Char)), true),
                    0xEF if b.get(j + 1) == Some(&0xBF)
                        && matches!(b.get(j + 2), Some(0xBE | 0xBF)) =>
                    {
                        return (Some((j, R::Char)), true)
                    }
                    _ => {}
                }
            }
        }
        i = end;
    }
    (None, high >= 0x80)
}

/// [`find_text_issue`] with the XML 1.1 rules (\[2a\] `RestrictedChar`
/// refused raw).
#[cold]
#[inline(never)]
fn find_text_issue11(b: &[u8]) -> Option<(usize, R)> {
    let char_issue = find_non_char11(b);
    let cdata_end = find(b, 0, b"]]>");
    match (char_issue, cdata_end) {
        (Some(a), Some(c)) if c < a => Some((c, R::CharData)),
        (Some(a), _) => Some((a, R::Char)),
        (None, Some(c)) => Some((c, R::CharData)),
        (None, None) => None,
    }
}

/// The first character of a comment that is not a \[2\] `Char`, the first
/// `--`, or a final `-` (\[15\] `Comment`), in one pass.
#[inline]
fn find_comment_issue(b: &[u8]) -> Option<(usize, R)> {
    let mut i = 0;
    while i < b.len() {
        let end = (i + 32).min(b.len());
        let mut flag = false;
        for &c in &b[i..end] {
            flag |= ((c < 0x20) & (c != b'\t') & (c != b'\n') & (c != b'\r'))
                | (c == 0xEF)
                | (c == b'-');
        }
        if flag {
            for j in i..end {
                match b[j] {
                    b'-' if j + 1 == b.len() || b[j + 1] == b'-' => return Some((j, R::Comment)),
                    b'\t' | b'\n' | b'\r' => {}
                    0..=0x1F => return Some((j, R::Char)),
                    0xEF if b.get(j + 1) == Some(&0xBF)
                        && matches!(b.get(j + 2), Some(0xBE | 0xBF)) =>
                    {
                        return Some((j, R::Char))
                    }
                    _ => {}
                }
            }
        }
        i = end;
    }
    None
}

/// [`find_comment_issue`] with the XML 1.1 rules.
#[cold]
#[inline(never)]
fn find_comment_issue11(b: &[u8]) -> Option<(usize, R)> {
    let char_issue = find_non_char11(b);
    let dashes = find(b, 0, b"--").or(if b.ends_with(b"-") {
        Some(b.len() - 1)
    } else {
        None
    });
    match (char_issue, dashes) {
        (Some(a), Some(d)) if d < a => Some((d, R::Comment)),
        (Some(a), _) => Some((a, R::Char)),
        (None, Some(d)) => Some((d, R::Comment)),
        (None, None) => None,
    }
}

#[cold]
fn comment_message(c: &[u8], i: usize, rule: R) -> String {
    match rule {
        R::Comment if c.get(i + 1) == Some(&b'-') => "'--' inside a comment".to_string(),
        R::Comment => "a comment ending in '-' ('--->')".to_string(),
        _ => non_char_message(c, i),
    }
}

/// At most 64 bytes of `b`, lossy, for a message.
#[cold]
fn shown(b: &[u8]) -> String {
    let mut end = b.len().min(64);
    while end < b.len() && (b[end] & 0xC0) == 0x80 {
        end += 1;
    }
    let s = String::from_utf8_lossy(&b[..end]);
    if end < b.len() {
        format!("{s}…")
    } else {
        s.into_owned()
    }
}

/// The character at `b[j]`, for a message.
#[cold]
fn shown_at(b: &[u8], j: usize) -> String {
    match b.get(j) {
        None => "the end of the tag".to_string(),
        Some(&c) if c < 0x80 => format!("'{}'", (c as char).escape_debug()),
        Some(_) => match decode_at(b, j) {
            Some((cp, _)) => match char::from_u32(cp) {
                Some(ch) => format!("'{}' ({})", ch.escape_debug(), code_point(cp)),
                None => code_point(cp),
            },
            None => "a byte that is not UTF-8".to_string(),
        },
    }
}

/// The token at the start of `b` (up to white space), for a message.
#[cold]
fn first_token(b: &[u8]) -> &[u8] {
    let end = b.iter().position(|&c| is_space(c)).unwrap_or(b.len());
    &b[..end]
}

#[cold]
fn code_point(v: u32) -> String {
    if v > 0x10FFFF {
        "a code point above U+10FFFF".to_string()
    } else {
        format!("U+{v:04X}")
    }
}

/// The message for the character at `b[j]` that is not allowed.
#[cold]
fn non_char_message(b: &[u8], j: usize) -> String {
    let cp = match b[j] {
        c if c < 0x80 => u32::from(c),
        _ => decode_at(b, j).map_or(0xFFFF, |(cp, _)| cp),
    };
    format!("{} is not allowed as a character here", code_point(cp))
}
