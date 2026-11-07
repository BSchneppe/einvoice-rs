//! Reusable driver that wires a [`quick_xml::Reader`] event stream into a
//! [`ValidationRuntime`].
//!
//! Two layers are provided:
//!
//! * [`drive_quick_xml`] / [`drive_quick_xml_in`] — turn-key, all you want is
//!   for the runtime's [`ValidationSink`] to receive every diagnostic. The
//!   helper calls [`ValidationRuntime::end_validation`] for you.
//! * [`drive_quick_xml_with`] / [`drive_quick_xml_with_in`] — callback-driven,
//!   for callers that need to interleave work between validator events
//!   (typed-document construction, source-span tracking, etc). The caller is
//!   responsible for `end_validation` on this path; see the
//!   [`ValidationEventHandler`] trait for hook ordering.
//!
//! The text is read through the crate's well-formedness layer
//! ([`crate::xml`]): a document that is not well-formed is refused with
//! [`DriveWithError::NotWellFormed`], and the runtime's
//! [`ParseLimits`](crate::xml::ParseLimits) are checked before anything of
//! the markup that breaks one is interned
//! ([`ValidationRuntime::set_parse_limits`]). A document type declaration in
//! the prolog is reported to [`ValidationEventHandler::on_doctype`] and not
//! processed; the XML declaration is checked and dropped. Comments and
//! processing instructions flow through the layer-2 hooks; layer 1 ignores
//! them via [`NoopHandler`].
//!
//! Namespace scoping (xmlns push/pop), `xsi:type`/`xsi:nil` discovery, and
//! [`NamespaceContextSnapshot`] construction are handled internally so the
//! caller does not have to.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use thiserror::Error;

use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::table::{XML_NAMESPACE, XSI_NAMESPACE};
use crate::schema::SchemaSet;
use crate::validation::errors::ValidationError;
use crate::validation::info::{SchemaInfo, SchemaValidity};
use crate::validation::runtime::ValidationRuntime;
use crate::validation::validator::ValidationSink;
use crate::xml::reader::{AttrKind, AttrSpan, Recheck, WfError, WfReader};
use crate::xml::{LimitExceeded, NotWellFormed};
use crate::xml_entity::resolve_general_ref;

// ── Public types ──────────────────────────────────────────────────────────

/// Final outcome of a successful drive call.
///
/// Validation diagnostics are reported through the runtime's
/// [`ValidationSink`], not through this struct.
#[derive(Debug, Clone)]
pub struct DriveOutcome {
    /// Validity of the root element after `validate_end_element`.
    /// `None` if the stream contained no elements.
    pub root_validity: Option<SchemaValidity>,
    /// Maximum element depth observed.
    pub max_depth: usize,
}

/// Errors raised by the layer-1 helpers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DriveError {
    /// An I/O error of the source (its own `std::io::Error`, also as
    /// [`source`](std::error::Error::source)).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The document is not well-formed or not namespace-well-formed (XML
    /// 1.0 / 1.1 and Namespaces in XML; see [`crate::xml`]): every
    /// refusal of the text — bytes that are not UTF-8, an unbound prefix,
    /// input that ends inside an element included — with its rule and byte
    /// position.
    #[error("{0}")]
    NotWellFormed(NotWellFormed),
    /// The document broke one of the run's
    /// [`ParseLimits`](crate::xml::ParseLimits).
    #[error("{0}")]
    LimitExceeded(LimitExceeded),
    /// `runtime.end_validation()` returned `Err` after the stream was driven.
    #[error("end_validation failed: {0}")]
    Validation(ValidationError),
}

/// Errors raised by the layer-2 helpers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DriveWithError<E> {
    /// An I/O error of the source (see [`DriveError::Io`]).
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
    /// The document is not well-formed or not namespace-well-formed (see
    /// [`DriveError::NotWellFormed`]).
    #[error("{0}")]
    NotWellFormed(NotWellFormed),
    /// The document broke one of the run's
    /// [`ParseLimits`](crate::xml::ParseLimits).
    #[error("{0}")]
    LimitExceeded(LimitExceeded),
    /// A handler hook returned an error.
    #[error("hook error")]
    Hook(E),
}

/// View of an element-start (or empty) event passed to handler hooks.
///
/// Borrowed slices live until the next `read_event_into` call, so hooks must
/// not retain references past their return.
#[derive(Clone, Copy)]
pub struct ElementStartView<'a> {
    pub local_name: &'a str,
    pub namespace_uri: &'a str,
    pub prefix: &'a str,
    /// Lexical value of `xsi:type`, if present.
    pub xsi_type: Option<&'a str>,
    /// Lexical value of `xsi:nil`, if present.
    pub xsi_nil: Option<&'a str>,
    /// Pre-built snapshot used for QName resolution inside the runtime.
    pub ns_context: &'a NamespaceContextSnapshot,
    /// xmlns declarations on THIS element (prefix, uri). Empty prefix is the
    /// default-namespace declaration.
    pub namespace_decls: &'a [(&'a str, &'a str)],
    /// `true` if this came from `Event::Empty`.
    pub is_empty: bool,
}

/// View of a non-xmlns attribute.
#[derive(Clone, Copy)]
pub struct AttributeView<'a> {
    pub local_name: &'a str,
    pub namespace_uri: &'a str,
    pub prefix: &'a str,
    /// Already unescaped (entity references resolved by quick-xml).
    pub value: &'a str,
}

/// Payload for [`ValidationEventHandler::after_end_of_attributes`].
#[non_exhaustive]
pub struct EndOfAttributesView<'a> {
    pub info: &'a SchemaInfo,
    /// The defaulted attributes of the element (Structures §3.4.4.2) — the
    /// optional attribute uses of its governing complex type with a `default`
    /// or `fixed` value that the element does not carry, which §3.4.5.1 adds
    /// to its \[attributes\] — with their value constraints' lexical forms as
    /// the schema writes them
    /// ([`ValidationRuntime::take_default_attributes`]). Empty for an element
    /// without a complex type.
    pub default_attributes: &'a [crate::validation::info::DefaultAttribute],
    /// Drained `take_deferred_attribute_results()` payload, in original
    /// attribute encounter order. Empty whenever no CTA reselection occurred.
    #[cfg(feature = "xsd11")]
    pub deferred_attribute_results: &'a [SchemaInfo],
}

/// A document type declaration in the prolog, passed to
/// [`ValidationEventHandler::on_doctype`].
///
/// Reported only once it is known to be well-formed: in the prolog, written
/// `<!DOCTYPE` S Name, with a well-formed external identifier and an internal
/// subset whose markup is balanced (literals, comments, processing
/// instructions, markup declarations, parameter-entity references). The
/// driver does not process it: it reads no external subset, and an entity the
/// internal subset declares is still undeclared when the document refers to
/// it (`EntityDeclared`).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DoctypeView<'a> {
    /// The declared root element name (\[28\] `doctypedecl ::= '<!DOCTYPE' S
    /// Name ...`).
    pub name: &'a str,
    /// Everything between `<!DOCTYPE` and the closing `>`, leading white
    /// space removed: the name, the external identifier and the internal
    /// subset as written.
    pub text: &'a str,
    /// Byte offset of the declaration's `<` in the input (a leading
    /// byte-order mark included), in the same system as
    /// [`ValidationEventHandler::on_element_start_offset`].
    pub byte_pos: usize,
}

/// What the runtime returned from `validate_end_element`: the element's
/// `[validity]` and `[validation attempted]` (Structures §3.3.5.1).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EndElementInfo {
    /// The element's `[validity]`.
    pub validity: SchemaValidity,
    /// The element's `[validation attempted]`: `full` when it and everything
    /// below it was strictly assessed, `none` when nothing was, `partial`
    /// otherwise — e.g. an element no declaration governs (laxly assessed)
    /// with an assessed attribute or descendant.
    pub validation_attempted: crate::validation::info::ValidationAttempted,
}

/// How a text/CDATA event was dispatched into the runtime.
#[derive(Debug, Clone, Copy)]
pub enum TextKind {
    /// `Event::Text` whose unescaped content is all whitespace —
    /// forwarded to `runtime.validate_whitespace`.
    Whitespace,
    /// `Event::Text` with non-whitespace content —
    /// forwarded to `runtime.validate_text`.
    Character,
    /// `Event::CData` — always forwarded to `runtime.validate_text`.
    CData,
}

// ── Handler trait ─────────────────────────────────────────────────────────

/// Handler invoked at each validator-event boundary.
///
/// Every method has a default implementation that does nothing, so a
/// handler that only cares about (say) end-of-element fires exactly that
/// one method.
///
/// Before the root element, a document type declaration in the prolog is
/// reported to `on_doctype` (comments and PIs there to their hooks).
///
/// Hook ordering for one element:
/// 1. `on_element_start_offset`
/// 2. `before_element`
/// 3. (internal) `runtime.validate_element`
/// 4. `after_element`
/// 5. For each non-xmlns attribute, in document order:
///    1. `before_attribute`
///    2. `skip_attribute` (not for attributes in the XML Schema instance
///       namespace)
///    3. (internal) `runtime.validate_attribute`, or `runtime.skip_attribute`
///       when the handler asked to skip it
///    4. `after_attribute`
/// 6. (internal) `runtime.validate_end_of_attributes`
/// 7. (internal, xsd11) `runtime.take_deferred_attribute_results`
/// 8. `after_end_of_attributes`
/// 9. Body events: `on_text`, `on_comment`, `on_processing_instruction`.
/// 10. On the closing event:
///     1. (internal) `runtime.validate_end_element`
///     2. `after_end_element`
///     3. `on_element_end_offset`
pub trait ValidationEventHandler {
    /// Caller's hook-error type. Reported through
    /// [`DriveWithError::Hook`].
    type Error;

    fn before_element(&mut self, _view: ElementStartView<'_>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn after_element(
        &mut self,
        _view: ElementStartView<'_>,
        _info: &SchemaInfo,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn before_attribute(&mut self, _view: AttributeView<'_>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn after_attribute(
        &mut self,
        _view: AttributeView<'_>,
        _info: &SchemaInfo,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Whether the driver should skip validating this attribute, if the
    /// element's governing type does not declare it. The default is `false`:
    /// every attribute is validated.
    ///
    /// When it returns `true`, the driver calls
    /// [`ValidationRuntime::skip_attribute`]: an attribute that no attribute
    /// use of the governing type matches is recorded without being assessed —
    /// treated as if a `processContents="skip"` attribute wildcard had matched
    /// it (that method documents what this means for the PSVI, identity
    /// constraints and ID/IDREF checks) — while a **declared** attribute is
    /// validated as usual, whatever the handler answered (a wildcard never
    /// governs an attribute an attribute use matches, Structures §3.4.4.2
    /// clause 2.1). `after_attribute` receives the resulting [`SchemaInfo`]:
    /// no type and `[validity]` notKnown for a skipped attribute.
    ///
    /// Called after `before_attribute`, for every attribute except
    /// namespace declarations and attributes in the XML Schema instance
    /// namespace (`xsi:type` and `xsi:nil` take effect at the element start,
    /// before any attribute event). The quick-xml drivers
    /// ([`drive_quick_xml_with`], and the typed build over it) honour it;
    /// [`drive_quick_xml`] runs no handler, and the tree-walking drivers
    /// (`drive_navigator`, `drive_buffer_document`) take none — they validate
    /// every attribute.
    fn skip_attribute(&mut self, _view: AttributeView<'_>) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn after_end_of_attributes(
        &mut self,
        _view: EndOfAttributesView<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// `depth` is the depth at which the element existed (1 = root close).
    fn after_end_element(
        &mut self,
        _info: &EndElementInfo,
        _depth: usize,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn on_text(&mut self, _kind: TextKind, _text: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn on_comment(&mut self, _text: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn on_processing_instruction(&mut self, _target: &str, _data: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    /// A document type declaration in the prolog. The default accepts it and
    /// does nothing — the declaration is not processed (see
    /// [`DoctypeView`]); return an error to refuse documents that carry one.
    ///
    /// A document type declaration anywhere but in the prolog — after or
    /// inside the root element, or a second one — is not well-formed and is
    /// refused by the driver ([`DriveWithError::NotWellFormed`]) without
    /// calling this hook.
    fn on_doctype(&mut self, _doctype: DoctypeView<'_>) -> Result<(), Self::Error> {
        Ok(())
    }

    /// The document's XML declaration, before anything else of the document
    /// (see [`XmlDeclaration`](crate::xml::XmlDeclaration)). The default
    /// accepts it; return an error to refuse, for example, an `encoding` the
    /// host did not decode the text from. Not called for a document without
    /// one.
    fn on_xml_declaration(
        &mut self,
        _declaration: &crate::xml::XmlDeclaration,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Byte offset into the input (a leading byte-order mark included) of
    /// the `<` of the element about to be reported to `before_element`.
    /// Default impl is a no-op; override only when building a span-aware
    /// DOM.
    fn on_element_start_offset(&mut self, _byte_pos: usize) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Byte offset into the input (a leading byte-order mark included) just
    /// past the `>` of the closing tag. Default impl is a no-op; override
    /// only when building a span-aware DOM.
    fn on_element_end_offset(&mut self, _byte_pos: usize) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Zero-sized handler whose every method is the trait default.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopHandler;

impl ValidationEventHandler for NoopHandler {
    type Error = Infallible;
}

// ── Layer 1: turn-key ─────────────────────────────────────────────────────

/// Drive a quick-xml stream into `runtime`, then call
/// `runtime.end_validation()`.
///
/// Validation diagnostics arrive through the sink the runtime was built with.
/// A document that is not well-formed, or breaks the runtime's parse limits,
/// is refused ([`DriveError::NotWellFormed`], [`DriveError::LimitExceeded`]).
/// A document type declaration in the prolog is accepted and not processed;
/// comments and PIs are dropped.
pub fn drive_quick_xml<R, S>(
    reader: R,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
) -> Result<DriveOutcome, DriveError>
where
    R: BufRead,
    S: ValidationSink,
{
    let mut buf = Vec::new();
    drive_quick_xml_in(reader, runtime, schema_set, &mut buf)
}

/// [`drive_quick_xml`] variant that reuses a caller-supplied buffer.
pub fn drive_quick_xml_in<R, S>(
    reader: R,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
    buf: &mut Vec<u8>,
) -> Result<DriveOutcome, DriveError>
where
    R: BufRead,
    S: ValidationSink,
{
    let mut handler = NoopHandler;
    let outcome = drive_quick_xml_with_in(reader, runtime, schema_set, &mut handler, buf).map_err(
        |e| match e {
            DriveWithError::Io(e) => DriveError::Io(e),
            DriveWithError::NotWellFormed(e) => DriveError::NotWellFormed(e),
            DriveWithError::LimitExceeded(e) => DriveError::LimitExceeded(e),
            DriveWithError::Hook(_) => unreachable!("NoopHandler is infallible"),
        },
    )?;
    runtime.end_validation().map_err(DriveError::Validation)?;
    Ok(outcome)
}

// ── Layer 2: handler-driven ───────────────────────────────────────────────

/// Drive a quick-xml stream into `runtime`, invoking `handler` at each
/// validator-event boundary.
///
/// **Does NOT call `runtime.end_validation()`.** The caller must do so after
/// any post-stream state collection (e.g. `runtime.schema_location_hints()`).
pub fn drive_quick_xml_with<R, S, H>(
    reader: R,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
    handler: &mut H,
) -> Result<DriveOutcome, DriveWithError<H::Error>>
where
    R: BufRead,
    S: ValidationSink,
    H: ValidationEventHandler,
{
    let mut buf = Vec::new();
    drive_quick_xml_with_in(reader, runtime, schema_set, handler, &mut buf)
}

/// [`drive_quick_xml_with`] variant that reuses a caller-supplied buffer.
pub fn drive_quick_xml_with_in<R, S, H>(
    reader: R,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
    handler: &mut H,
    buf: &mut Vec<u8>,
) -> Result<DriveOutcome, DriveWithError<H::Error>>
where
    R: BufRead,
    S: ValidationSink,
    H: ValidationEventHandler,
{
    // Every check of the shared well-formedness layer, and the run's parse
    // limits, hold for an event before it reaches this loop.
    let mut xml_reader = WfReader::new(reader, runtime.parse_limits(), false);

    // prefix bytes -> stack of URI strings (top-of-stack = current binding)
    let mut prefix_map: HashMap<Vec<u8>, Vec<String>> = HashMap::new();
    // Always-in-scope xml prefix (XML Namespaces §3).
    prefix_map
        .entry(b"xml".to_vec())
        .or_default()
        .push(XML_NAMESPACE.to_string());
    // Default-namespace seed; explicit declarations overwrite the top-of-stack.
    prefix_map
        .entry(Vec::new())
        .or_default()
        .push(String::new());

    // Per-element list of prefixes that need popping at end-of-element.
    let mut scope_stack: Vec<Vec<Vec<u8>>> = Vec::new();

    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut root_validity: Option<SchemaValidity> = None;
    // Character data is reported in pieces: a run containing `&amp;` arrives as
    // Text / GeneralRef / Text. Collect a whole run here so it is classified and
    // validated as one chunk, the way a single unsplit text event would be.
    let mut pending_text = String::new();

    // The reader clears the buffer itself before each read.
    buf.clear();

    loop {
        let event = xml_reader.read_event_into(buf);
        let event_start = xml_reader.event_start() as usize;

        // A run of character data containing `&amp;` or `&#65;` is reported as
        // Text / GeneralRef / Text, so gather the whole run before validating
        // it — splitting it would classify the pieces separately and report a
        // single ill-placed run more than once.
        match &event {
            Ok(Event::Text(e)) if depth > 0 => {
                // XML 1.0 / 1.1 line-end normalization (§2.11).
                pending_text.push_str(
                    &e.xml_content(xml_reader.quick_xml_version())
                        .map_err(|_| drive_error(xml_reader.recheck().utf8()))?,
                );
                continue;
            }
            Ok(Event::GeneralRef(e)) if depth > 0 => {
                pending_text.push_str(
                    &resolve_general_ref(e)
                        .map_err(|e| drive_error(xml_reader.recheck().decoding(e)))?,
                );
                continue;
            }
            _ => flush_text_run(&mut pending_text, runtime, handler)?,
        }

        match event {
            Ok(Event::Start(ref e)) => {
                handle_start_or_empty(
                    e,
                    false,
                    event_start,
                    &xml_reader,
                    runtime,
                    schema_set,
                    handler,
                    &mut prefix_map,
                    &mut scope_stack,
                    &mut depth,
                    &mut max_depth,
                    &mut root_validity,
                )?;
            }
            Ok(Event::Empty(ref e)) => {
                handle_start_or_empty(
                    e,
                    true,
                    event_start,
                    &xml_reader,
                    runtime,
                    schema_set,
                    handler,
                    &mut prefix_map,
                    &mut scope_stack,
                    &mut depth,
                    &mut max_depth,
                    &mut root_validity,
                )?;
            }
            Ok(Event::End(_)) => {
                // Streaming consumers receive only `[validity]` and `[validation
                // attempted]` per element, so use the value-free end path (no
                // per-element SchemaInfo).
                let (end_validity, validation_attempted) = runtime.validate_end_element_verdict();
                let end = EndElementInfo {
                    validity: end_validity,
                    validation_attempted,
                };
                handler
                    .after_end_element(&end, depth)
                    .map_err(DriveWithError::Hook)?;
                let end_pos = xml_reader.buffer_position() as usize;
                handler
                    .on_element_end_offset(end_pos)
                    .map_err(DriveWithError::Hook)?;
                if depth == 1 {
                    root_validity = Some(end_validity);
                }
                pop_xmlns_scope(&mut prefix_map, &mut scope_stack);
                depth = depth.saturating_sub(1);
            }
            Ok(Event::CData(ref e)) if depth > 0 => {
                let s = std::str::from_utf8(e.as_ref())
                    .map_err(|_| drive_error(xml_reader.recheck().utf8()))?;
                runtime.validate_text(s);
                handler
                    .on_text(TextKind::CData, s)
                    .map_err(DriveWithError::Hook)?;
            }
            Ok(Event::Text(_) | Event::GeneralRef(_) | Event::CData(_)) => {
                // Outside any element: white space (the reader refuses
                // anything else there) — significant for neither validator
                // nor handler.
            }
            Ok(Event::Comment(ref e)) => {
                let s = std::str::from_utf8(e.as_ref())
                    .map_err(|_| drive_error(xml_reader.recheck().utf8()))?;
                handler.on_comment(s).map_err(DriveWithError::Hook)?;
            }
            Ok(Event::PI(ref e)) => {
                let raw = std::str::from_utf8(e.as_ref())
                    .map_err(|_| drive_error(xml_reader.recheck().utf8()))?;
                let (target, data) = parse_pi_content(raw);
                handler
                    .on_processing_instruction(target, data)
                    .map_err(DriveWithError::Hook)?;
            }
            Ok(Event::DocType(ref e)) => {
                // In the prolog (the reader refuses one anywhere else). The
                // keyword is checked on the markup before the hook sees it.
                let text = std::str::from_utf8(e.as_ref())
                    .map_err(|_| drive_error(xml_reader.recheck().utf8()))?
                    .to_string();
                xml_reader.confirm_doctype(buf).map_err(drive_error)?;
                let name_end = text
                    .find([' ', '\t', '\n', '\r', '['])
                    .unwrap_or(text.len());
                handler
                    .on_doctype(DoctypeView {
                        name: &text[..name_end],
                        text: &text,
                        byte_pos: event_start,
                    })
                    .map_err(DriveWithError::Hook)?;
            }
            Ok(Event::Decl(_)) => {
                if let Some(declaration) = xml_reader.declaration() {
                    handler
                        .on_xml_declaration(declaration)
                        .map_err(DriveWithError::Hook)?;
                }
            }
            Ok(Event::Eof) => {
                if depth != 0 {
                    // The reader refuses input that ends inside an element
                    // before it returns `Eof`.
                    return Err(drive_error(xml_reader.recheck().truncated(depth)));
                }
                break;
            }
            Err(e) => return Err(drive_error(e)),
        }
    }

    Ok(DriveOutcome {
        root_validity,
        max_depth,
    })
}

// ── Internals ─────────────────────────────────────────────────────────────

/// The driver's error for an error of the well-formedness layer.
fn drive_error<E>(e: WfError) -> DriveWithError<E> {
    match e {
        WfError::Io(e) => DriveWithError::Io(e),
        WfError::NotWellFormed(e) => DriveWithError::NotWellFormed(e),
        WfError::Limit(e) => DriveWithError::LimitExceeded(e),
    }
}

/// Validate and forward an accumulated run of character data, then reset it.
///
/// Whitespace-only runs go through the whitespace path, which element-only
/// content models ignore; anything else is character content.
fn flush_text_run<S, H>(
    pending: &mut String,
    runtime: &mut ValidationRuntime<'_, S>,
    handler: &mut H,
) -> Result<(), DriveWithError<H::Error>>
where
    S: ValidationSink,
    H: ValidationEventHandler,
{
    if pending.is_empty() {
        return Ok(());
    }

    let kind = if pending.chars().all(|c| c.is_whitespace()) {
        runtime.validate_whitespace(pending);
        TextKind::Whitespace
    } else {
        runtime.validate_text(pending);
        TextKind::Character
    };
    let result = handler.on_text(kind, pending).map_err(DriveWithError::Hook);
    pending.clear();
    result
}

#[allow(clippy::too_many_arguments)]
fn handle_start_or_empty<R, S, H>(
    e: &BytesStart<'_>,
    is_empty: bool,
    event_start: usize,
    xml_reader: &WfReader<R>,
    runtime: &mut ValidationRuntime<'_, S>,
    schema_set: &SchemaSet,
    handler: &mut H,
    prefix_map: &mut HashMap<Vec<u8>, Vec<String>>,
    scope_stack: &mut Vec<Vec<Vec<u8>>>,
    depth: &mut usize,
    max_depth: &mut usize,
    root_validity: &mut Option<SchemaValidity>,
) -> Result<(), DriveWithError<H::Error>>
where
    R: BufRead,
    S: ValidationSink,
    H: ValidationEventHandler,
{
    *depth += 1;
    if *depth > *max_depth {
        *max_depth = *depth;
    }

    // 1. Push xmlns scope, collect declarations as (prefix_string, uri_string).
    // The tag's attributes, as the reader found them while checking it.
    let tag: &[u8] = e;
    let spans = xml_reader.attributes();
    let version = xml_reader.quick_xml_version();
    // The reader checked the tag (UTF-8, bound prefixes, references): `at`
    // reports what still fails as the refusal it is (see `Recheck`).
    let at = xml_reader.recheck();

    let ns_decls_owned = match push_xmlns_scope(tag, spans, version, at, prefix_map, scope_stack) {
        Ok(v) => v,
        Err(err) => {
            // Scope was not pushed, so just unwind depth.
            *depth -= 1;
            return Err(err);
        }
    };
    let ns_decls: Vec<(&str, &str)> = ns_decls_owned
        .iter()
        .map(|(p, u)| (p.as_str(), u.as_str()))
        .collect();

    // 2. Resolve element name + scan xsi:type / xsi:nil.
    let (local_name, namespace_uri, elem_prefix) = match resolve_element_qname(e, at, prefix_map) {
        Ok(v) => v,
        Err(err) => {
            pop_xmlns_scope(prefix_map, scope_stack);
            *depth -= 1;
            return Err(err);
        }
    };
    let (xsi_type, xsi_nil) = match scan_xsi_attributes(tag, spans, version, at, prefix_map) {
        Ok(v) => v,
        Err(err) => {
            pop_xmlns_scope(prefix_map, scope_stack);
            *depth -= 1;
            return Err(err);
        }
    };

    let ns_ctx = build_ns_context(prefix_map, schema_set);

    let view = ElementStartView {
        local_name: &local_name,
        namespace_uri: &namespace_uri,
        prefix: &elem_prefix,
        xsi_type: xsi_type.as_deref(),
        xsi_nil: xsi_nil.as_deref(),
        ns_context: &ns_ctx,
        namespace_decls: &ns_decls,
        is_empty,
    };

    // 3. Spans + before_element.
    if let Err(err) = handler.on_element_start_offset(event_start) {
        pop_xmlns_scope(prefix_map, scope_stack);
        *depth -= 1;
        return Err(DriveWithError::Hook(err));
    }
    if let Err(err) = handler.before_element(view) {
        pop_xmlns_scope(prefix_map, scope_stack);
        *depth -= 1;
        return Err(DriveWithError::Hook(err));
    }

    // 4. Element validation + after_element.
    let info = runtime.validate_element(
        &local_name,
        &namespace_uri,
        xsi_type.as_deref(),
        xsi_nil.as_deref(),
        &ns_ctx,
    );
    handler
        .after_element(view, &info)
        .map_err(DriveWithError::Hook)?;

    // 5. Attributes.
    for span in spans {
        if span.kind != AttrKind::Attribute {
            continue;
        }
        let prefix_bytes = span.prefix(tag);
        let utf8 = |_| drive_error(at.utf8());
        let attr_local = std::str::from_utf8(span.local(tag)).map_err(utf8)?;
        let attr_prefix = std::str::from_utf8(prefix_bytes).map_err(utf8)?;
        let attr_ns = if prefix_bytes.is_empty() {
            String::new()
        } else {
            match prefix_map.get(prefix_bytes).and_then(|stack| stack.last()) {
                Some(uri) if !uri.is_empty() => uri.clone(),
                _ => return Err(drive_error(at.unbound(prefix_bytes))),
            }
        };
        let value = span
            .value(tag, version)
            .map_err(|e| drive_error(at.decoding(e)))?;

        let av = AttributeView {
            local_name: attr_local,
            namespace_uri: &attr_ns,
            prefix: attr_prefix,
            value: value.as_ref(),
        };
        handler.before_attribute(av).map_err(DriveWithError::Hook)?;
        let skip =
            attr_ns != XSI_NAMESPACE && handler.skip_attribute(av).map_err(DriveWithError::Hook)?;
        let attr_info = if skip {
            runtime.skip_attribute(attr_local, &attr_ns, value.as_ref())
        } else {
            runtime.validate_attribute(attr_local, &attr_ns, value.as_ref())
        };
        handler
            .after_attribute(av, &attr_info)
            .map_err(DriveWithError::Hook)?;
    }

    // 6. End-of-attributes.
    let eoa_info = runtime.validate_end_of_attributes();
    #[cfg(feature = "xsd11")]
    let deferred = runtime.take_deferred_attribute_results();
    let defaults = runtime.take_default_attributes();
    let eoa_view = EndOfAttributesView {
        info: &eoa_info,
        default_attributes: &defaults,
        #[cfg(feature = "xsd11")]
        deferred_attribute_results: &deferred,
    };
    handler
        .after_end_of_attributes(eoa_view)
        .map_err(DriveWithError::Hook)?;

    // 7. For empty elements, close inline.
    if is_empty {
        // Streaming consumers receive only `[validity]` and `[validation
        // attempted]` per element (the handler hook takes an `EndElementInfo`),
        // so use the value-free end path.
        let (end_validity, validation_attempted) = runtime.validate_end_element_verdict();
        let end = EndElementInfo {
            validity: end_validity,
            validation_attempted,
        };
        handler
            .after_end_element(&end, *depth)
            .map_err(DriveWithError::Hook)?;
        let end_pos = xml_reader.buffer_position() as usize;
        handler
            .on_element_end_offset(end_pos)
            .map_err(DriveWithError::Hook)?;
        if *depth == 1 {
            *root_validity = Some(end_validity);
        }
        pop_xmlns_scope(prefix_map, scope_stack);
        *depth -= 1;
    }

    Ok(())
}

/// Collect xmlns / xmlns:* declarations of the tag into `prefix_map`.
/// Returns the (prefix, uri) declarations so they can be exposed to handlers.
fn push_xmlns_scope<E>(
    tag: &[u8],
    spans: &[AttrSpan],
    version: quick_xml::XmlVersion,
    at: Recheck,
    prefix_map: &mut HashMap<Vec<u8>, Vec<String>>,
    scope_stack: &mut Vec<Vec<Vec<u8>>>,
) -> Result<Vec<(String, String)>, DriveWithError<E>> {
    let mut declared: Vec<Vec<u8>> = Vec::new();
    let mut decls_owned: Vec<(String, String)> = Vec::new();

    for span in spans {
        if span.kind == AttrKind::Attribute {
            continue;
        }
        let prefix_bytes = span.prefix(tag);
        let prefix_str = std::str::from_utf8(prefix_bytes)
            .map_err(|_| drive_error(at.utf8()))?
            .to_string();
        let uri = span
            .value(tag, version)
            .map_err(|e| drive_error(at.decoding(e)))?
            .into_owned();
        prefix_map
            .entry(prefix_bytes.to_vec())
            .or_default()
            .push(uri.clone());
        declared.push(prefix_bytes.to_vec());
        decls_owned.push((prefix_str, uri));
    }

    scope_stack.push(declared);
    Ok(decls_owned)
}

fn pop_xmlns_scope(
    prefix_map: &mut HashMap<Vec<u8>, Vec<String>>,
    scope_stack: &mut Vec<Vec<Vec<u8>>>,
) {
    if let Some(declared) = scope_stack.pop() {
        for prefix in declared {
            if let Some(stack) = prefix_map.get_mut(&prefix) {
                stack.pop();
                if stack.is_empty() {
                    prefix_map.remove(&prefix);
                }
            }
        }
    }
}

fn resolve_element_qname<E>(
    e: &BytesStart<'_>,
    at: Recheck,
    prefix_map: &HashMap<Vec<u8>, Vec<String>>,
) -> Result<(String, String, String), DriveWithError<E>> {
    let name = e.name();
    let (prefix_bytes, local_bytes) = split_prefix_local(name.as_ref());
    let local = std::str::from_utf8(local_bytes)
        .map_err(|_| drive_error(at.utf8()))?
        .to_string();
    let prefix = std::str::from_utf8(prefix_bytes)
        .map_err(|_| drive_error(at.utf8()))?
        .to_string();
    let namespace = if prefix_bytes.is_empty() {
        prefix_map
            .get(prefix_bytes)
            .and_then(|stack| stack.last())
            .cloned()
            .unwrap_or_default()
    } else {
        match prefix_map.get(prefix_bytes).and_then(|stack| stack.last()) {
            Some(uri) if !uri.is_empty() => uri.clone(),
            _ => return Err(drive_error(at.unbound(prefix_bytes))),
        }
    };
    Ok((local, namespace, prefix))
}

fn scan_xsi_attributes<E>(
    tag: &[u8],
    spans: &[AttrSpan],
    version: quick_xml::XmlVersion,
    at: Recheck,
    prefix_map: &HashMap<Vec<u8>, Vec<String>>,
) -> Result<(Option<String>, Option<String>), DriveWithError<E>> {
    let mut xsi_type: Option<String> = None;
    let mut xsi_nil: Option<String> = None;
    for span in spans {
        if span.kind != AttrKind::Attribute {
            continue;
        }
        let prefix_bytes = span.prefix(tag);
        if prefix_bytes.is_empty() {
            continue;
        }
        let ns_uri = match prefix_map.get(prefix_bytes).and_then(|s| s.last()) {
            Some(uri) => uri.as_str(),
            None => continue,
        };
        if ns_uri != XSI_NAMESPACE {
            continue;
        }
        let local = std::str::from_utf8(span.local(tag)).map_err(|_| drive_error(at.utf8()))?;
        match local {
            "type" => {
                xsi_type = Some(
                    span.value(tag, version)
                        .map_err(|e| drive_error(at.decoding(e)))?
                        .into_owned(),
                )
            }
            "nil" => {
                xsi_nil = Some(
                    span.value(tag, version)
                        .map_err(|e| drive_error(at.decoding(e)))?
                        .into_owned(),
                )
            }
            _ => {}
        }
    }
    Ok((xsi_type, xsi_nil))
}

fn build_ns_context(
    prefix_map: &HashMap<Vec<u8>, Vec<String>>,
    schema_set: &SchemaSet,
) -> NamespaceContextSnapshot {
    let mut snapshot = NamespaceContextSnapshot::default();

    for (prefix_bytes, stack) in prefix_map {
        let uri = match stack.last() {
            Some(s) => s,
            None => continue,
        };
        if prefix_bytes.is_empty() {
            // Default namespace; skip the empty seed binding.
            if uri.is_empty() {
                continue;
            }
            snapshot.default_ns = Some(schema_set.name_table.add(uri));
        } else if let Ok(prefix_str) = std::str::from_utf8(prefix_bytes) {
            // Skip the always-in-scope xml prefix and any xmlns binding —
            // the runtime treats them as implicit.
            if prefix_str == "xml" || prefix_str == "xmlns" || uri.is_empty() {
                continue;
            }
            let prefix_id = schema_set.name_table.add(prefix_str);
            let uri_id = schema_set.name_table.add(uri);
            snapshot.bindings.push((prefix_id, uri_id));
        }
    }

    snapshot
}

fn split_prefix_local(name: &[u8]) -> (&[u8], &[u8]) {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => (&name[..pos], &name[pos + 1..]),
        None => (b"", name),
    }
}

fn parse_pi_content(raw: &str) -> (&str, &str) {
    // `PI ::= '<?' PITarget (S (Char* - (Char* '?>')))? '?>'` (XML 1.0 §2.6):
    // only the `S` separating the target from the data is not data. Whatever
    // follows it — trailing whitespace included — is the data verbatim.
    let raw = raw.trim_start();
    match raw.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&raw[..pos], raw[pos..].trim_start()),
        None => (raw, ""),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::load_and_process_schema;
    use crate::validation::{
        CollectingValidationSink, SchemaValidator, ValidationFlags, ValidationWarning,
    };

    fn load_schema(xsd: &str) -> SchemaSet {
        let mut ss = SchemaSet::new();
        load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut ss, None).expect("schema parse");
        ss
    }

    fn run(xsd: &str, instance: &str) -> (DriveOutcome, Vec<String>) {
        let schema_set = load_schema(xsd);
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let outcome =
            drive_quick_xml(instance.as_bytes(), &mut runtime, &schema_set).expect("drive failed");
        (outcome, errors.iter().map(|e| e.to_string()).collect())
    }

    #[test]
    fn simple_valid_root() {
        let (outcome, errors) = run(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
            "<root>hello</root>",
        );
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
        assert!(matches!(outcome.root_validity, Some(SchemaValidity::Valid)));
        assert_eq!(outcome.max_depth, 1);
    }

    #[test]
    fn empty_root_element() {
        let (outcome, _errors) = run(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType/>
                </xs:element>
            </xs:schema>"#,
            "<root/>",
        );
        assert!(matches!(outcome.root_validity, Some(SchemaValidity::Valid)));
        assert_eq!(outcome.max_depth, 1);
    }

    #[test]
    fn unexpected_eof_open_element() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        // Truncated stream: open tag with no close.
        let res = drive_quick_xml("<root>".as_bytes(), &mut runtime, &schema_set);
        match res {
            Err(DriveError::NotWellFormed(e)) => {
                assert_eq!(e.rule(), crate::xml::WellFormednessRule::Truncated);
                assert!(e.message().contains("1 element"), "{e}");
            }
            other => panic!("expected NotWellFormed(Truncated), got {other:?}"),
        }
    }

    #[test]
    fn unbound_attribute_prefix_errors() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:anyAttribute processContents="skip"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let res = drive_quick_xml(
            r#"<root nope:x="1"/>"#.as_bytes(),
            &mut runtime,
            &schema_set,
        );
        match res {
            Err(DriveError::NotWellFormed(e)) => {
                assert_eq!(e.rule(), crate::xml::WellFormednessRule::PrefixDeclared);
                assert!(e.message().contains("'nope'"), "{e}");
            }
            other => panic!("expected NotWellFormed(PrefixDeclared), got {other:?}"),
        }
    }

    #[test]
    fn buffer_reuse_across_calls() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut buf = Vec::new();
        for _ in 0..2 {
            let mut errors = Vec::new();
            let mut warnings: Vec<ValidationWarning> = Vec::new();
            let sink = CollectingValidationSink {
                errors: &mut errors,
                warnings: &mut warnings,
            };
            let mut runtime = validator.start_run(sink);
            let outcome = drive_quick_xml_in(
                "<root>x</root>".as_bytes(),
                &mut runtime,
                &schema_set,
                &mut buf,
            )
            .expect("drive ok");
            assert!(matches!(outcome.root_validity, Some(SchemaValidity::Valid)));
            assert!(errors.is_empty());
        }
    }

    /// Handler that records every text run the driver reports, tagged with the
    /// dispatch path it took.
    struct TextRuns {
        runs: Vec<(&'static str, String)>,
    }

    impl ValidationEventHandler for TextRuns {
        type Error = Infallible;
        fn on_text(&mut self, kind: TextKind, text: &str) -> Result<(), Self::Error> {
            let tag = match kind {
                TextKind::Whitespace => "ws",
                TextKind::Character => "chars",
                TextKind::CData => "cdata",
            };
            self.runs.push((tag, text.to_string()));
            Ok(())
        }
    }

    fn text_runs(xsd: &str, instance: &str) -> (Vec<(&'static str, String)>, Vec<String>) {
        let schema_set = load_schema(xsd);
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let mut handler = TextRuns { runs: Vec::new() };
        drive_quick_xml_with(instance.as_bytes(), &mut runtime, &schema_set, &mut handler)
            .expect("drive ok");
        (handler.runs, errors.iter().map(|e| e.to_string()).collect())
    }

    #[test]
    fn general_references_rejoin_the_surrounding_text_run() {
        let (runs, errors) = text_runs(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
            "<root>a &amp; b&#33;</root>",
        );
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
        assert_eq!(runs, vec![("chars", "a & b!".to_string())]);
    }

    #[test]
    fn character_reference_to_space_stays_whitespace() {
        // Element-only content ignores whitespace, so a run that resolves to
        // nothing but spaces must not be reported as character content.
        let (runs, errors) = text_runs(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="b" type="xs:string"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
            "<root> &#32; <b>x</b></root>",
        );
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
        assert_eq!(runs[0], ("ws", "   ".to_string()));
    }

    #[test]
    fn entity_in_element_only_content_is_reported_once() {
        let (_runs, errors) = text_runs(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="b" type="xs:string"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
            "<root> &amp; <b>x</b></root>",
        );
        assert_eq!(
            errors.len(),
            1,
            "one ill-placed text run must yield one error, got {errors:?}"
        );
    }

    #[test]
    fn unknown_entity_is_a_parse_error() {
        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root" type="xs:string"/>
            </xs:schema>"#,
        );
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);
        let res = drive_quick_xml("<root>&nope;</root>".as_bytes(), &mut runtime, &schema_set);
        assert!(
            matches!(&res, Err(DriveError::NotWellFormed(e))
                if e.rule() == crate::xml::WellFormednessRule::EntityDeclared),
            "got {res:?}"
        );
    }

    #[test]
    fn attribute_values_are_xml_normalized() {
        // XML attribute-value normalization (XML 1.0 §3.3.3) turns the literal
        // tab and newline into spaces before the value reaches the validator,
        // so the enumeration below matches.
        let (outcome, errors) = run(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:attribute name="a">
                            <xs:simpleType>
                                <xs:restriction base="xs:string">
                                    <xs:enumeration value="x   y &amp; z"/>
                                </xs:restriction>
                            </xs:simpleType>
                        </xs:attribute>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
            "<root a='x	
 y &amp; z'/>",
        );
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
        assert!(matches!(outcome.root_validity, Some(SchemaValidity::Valid)));
    }

    #[test]
    fn comment_and_pi_forwarded_to_handler() {
        struct Capture {
            comments: Vec<String>,
            pis: Vec<(String, String)>,
        }
        impl ValidationEventHandler for Capture {
            type Error = Infallible;
            fn on_comment(&mut self, text: &str) -> Result<(), Self::Error> {
                self.comments.push(text.to_string());
                Ok(())
            }
            fn on_processing_instruction(
                &mut self,
                target: &str,
                data: &str,
            ) -> Result<(), Self::Error> {
                self.pis.push((target.to_string(), data.to_string()));
                Ok(())
            }
        }

        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType/>
                </xs:element>
            </xs:schema>"#,
        );
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);

        let mut handler = Capture {
            comments: Vec::new(),
            pis: Vec::new(),
        };
        let _ = drive_quick_xml_with(
            "<root><!-- hi --><?pi target data?></root>".as_bytes(),
            &mut runtime,
            &schema_set,
            &mut handler,
        )
        .expect("drive ok");
        assert_eq!(handler.comments, vec![" hi ".to_string()]);
        assert_eq!(
            handler.pis,
            vec![("pi".to_string(), "target data".to_string())]
        );
    }

    #[test]
    fn span_offsets_bracket_each_element() {
        struct Spans {
            spans: Vec<(usize, usize)>,
            stack: Vec<usize>,
        }
        impl ValidationEventHandler for Spans {
            type Error = Infallible;
            fn on_element_start_offset(&mut self, byte_pos: usize) -> Result<(), Self::Error> {
                self.stack.push(byte_pos);
                Ok(())
            }
            fn on_element_end_offset(&mut self, byte_pos: usize) -> Result<(), Self::Error> {
                let start = self.stack.pop().expect("balanced span stack");
                self.spans.push((start, byte_pos));
                Ok(())
            }
        }

        let schema_set = load_schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="root">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="b" type="xs:string"/>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#,
        );
        let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
        let mut errors = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let sink = CollectingValidationSink {
            errors: &mut errors,
            warnings: &mut warnings,
        };
        let mut runtime = validator.start_run(sink);

        let mut handler = Spans {
            spans: Vec::new(),
            stack: Vec::new(),
        };
        let xml = "<root><b>hi</b></root>";
        drive_quick_xml_with(xml.as_bytes(), &mut runtime, &schema_set, &mut handler)
            .expect("drive ok");
        // Inner <b> closes before <root>; spans are captured in close order.
        assert_eq!(handler.spans.len(), 2);
        for (start, end) in &handler.spans {
            assert!(end > start);
            assert!(*end <= xml.len());
        }
    }
}
