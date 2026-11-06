//! Validation error types for instance validation
//!
//! This module provides spec-aligned error codes for XML instance validation
//! against XSD schemas. Error codes follow the XSD specification anchors:
//!
//! - `cvc-*` - Instance validation constraints (e.g., `cvc-elt`, `cvc-type`)
//! - `cos-*` - Component constraints (e.g., `cos-valid-default`)
//! - `src-*` - Schema representation constraints (e.g., `src-element`)

use std::sync::{Arc, OnceLock};

use crate::error::FacetError;
use crate::parser::location::SourceLocation;
use crate::types::validators::ValidationError as TypeValidationError;

// ---------------------------------------------------------------------------
// Subject and path of a diagnostic
// ---------------------------------------------------------------------------

/// An expanded name: an optional namespace name and a local name.
///
/// The parts are shared, reference-counted strings, so a name outlives the
/// validation run that produced it and cloning it does not copy them. The
/// representation is private: read the parts with
/// [`namespace`](Self::namespace) and [`local_name`](Self::local_name).
/// A name in no namespace has no namespace — an empty namespace given to
/// [`new`](Self::new) is no namespace, for equality and hashing too.
///
/// `Display` writes the XPath 3.0 braced-URI form `Q{uri}local`
/// (`Q{}local` for a name in no namespace). XPath 3.0's `BracedURILiteral`
/// cannot contain `{` or `}`: a namespace name that does is written as it is,
/// and the result is then not a parsable name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExpandedName {
    namespace: Option<Arc<str>>,
    local_name: Arc<str>,
}

impl ExpandedName {
    /// An expanded name. An empty `namespace` counts as no namespace.
    pub fn new(namespace: Option<&str>, local_name: &str) -> Self {
        ExpandedName {
            namespace: namespace.filter(|ns| !ns.is_empty()).map(Arc::from),
            local_name: Arc::from(local_name),
        }
    }

    /// An expanded name over already shared parts (no copy).
    pub(crate) fn from_shared(namespace: Option<Arc<str>>, local_name: Arc<str>) -> Self {
        ExpandedName {
            namespace: namespace.filter(|ns| !ns.is_empty()),
            local_name,
        }
    }

    /// The namespace name; `None` for a name in no namespace.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// The namespace name, `""` for a name in no namespace.
    pub fn namespace_uri(&self) -> &str {
        self.namespace.as_deref().unwrap_or("")
    }

    /// The local name.
    pub fn local_name(&self) -> &str {
        &self.local_name
    }
}

impl std::fmt::Display for ExpandedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Q{{{}}}{}", self.namespace_uri(), self.local_name)
    }
}

/// The information item a validation error or warning is about.
///
/// For a diagnostic the runtime reports, this is the item it was processing
/// when it found the problem: the attribute while an attribute is being
/// validated (also when the attributes of an element with type alternatives
/// are validated after the selection), otherwise the element being validated
/// — the element just opened, or the element being closed during its
/// end-of-element checks; the same element
/// [`element_path`](ValidationError::element_path) names. So an unexpected
/// child (`cvc-complex-type.2.4`, found while advancing the parent's content
/// model) is reported about the parent, and the message names the child; a
/// duplicate key value is reported about the element the selector selected.
/// A diagnostic made later is about the item recorded when it was deferred:
/// an unresolved IDREF (`cvc-id.1`, reported by `end_validation`) is about
/// the attribute or element that carried it, a failed keyref retried at an
/// ancestor about the keyref's element, a nested element's assertion
/// (evaluated when the outermost asserted element closes) about that nested
/// element.
///
/// `Display` writes the name, preceded by `@` for an attribute.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValidationSubject {
    /// An element information item.
    Element(ExpandedName),
    /// An attribute information item.
    Attribute(ExpandedName),
}

impl ValidationSubject {
    /// The subject's expanded name.
    pub fn name(&self) -> &ExpandedName {
        match self {
            ValidationSubject::Element(name) | ValidationSubject::Attribute(name) => name,
        }
    }
}

impl std::fmt::Display for ValidationSubject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationSubject::Element(name) => write!(f, "{name}"),
            ValidationSubject::Attribute(name) => write!(f, "@{name}"),
        }
    }
}

/// One step of an [`InstancePath`]. Build one with [`element`](Self::element)
/// or [`attribute`](Self::attribute).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PathStep {
    /// A child element, `Q{uri}local[position]`: `position` is 1-based among
    /// the preceding siblings with the same expanded name, plus one — the
    /// position the XPath step `Q{uri}local[position]` selects.
    #[non_exhaustive]
    Element {
        /// The element's expanded name.
        name: ExpandedName,
        /// The 1-based position among same-named siblings.
        position: usize,
    },
    /// An attribute, `@Q{uri}local` (always the last step).
    #[non_exhaustive]
    Attribute {
        /// The attribute's expanded name.
        name: ExpandedName,
    },
}

impl PathStep {
    /// An element step: the `position`-th child element named `name`.
    pub fn element(name: ExpandedName, position: usize) -> Self {
        PathStep::Element { name, position }
    }

    /// An attribute step.
    pub fn attribute(name: ExpandedName) -> Self {
        PathStep::Attribute { name }
    }

    /// The step's expanded name.
    pub fn name(&self) -> &ExpandedName {
        match self {
            PathStep::Element { name, .. } | PathStep::Attribute { name } => name,
        }
    }

    /// The position of an element step; `None` for an attribute step.
    pub fn position(&self) -> Option<usize> {
        match self {
            PathStep::Element { position, .. } => Some(*position),
            PathStep::Attribute { .. } => None,
        }
    }
}

impl std::fmt::Display for PathStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathStep::Element { name, position } => write!(f, "{name}[{position}]"),
            PathStep::Attribute { name } => write!(f, "@{name}"),
        }
    }
}

/// A node of an [`InstancePath`]: a step and the path to its parent.
struct PathNode {
    step: PathStep,
    parent: Option<Arc<PathNode>>,
    /// Number of steps from the top down to this one.
    len: usize,
}

impl Drop for PathNode {
    /// Unlink the parents iteratively: a path can be as deep as the document,
    /// and the default recursive drop of a long chain would exhaust the stack.
    fn drop(&mut self) {
        let mut parent = self.parent.take();
        while let Some(node) = parent {
            match Arc::try_unwrap(node) {
                Ok(mut node) => parent = node.parent.take(),
                Err(_) => break,
            }
        }
    }
}

/// A namespace-aware, positional path from the document to an element or
/// attribute of the instance, as XPath 3.0 steps.
///
/// `Display` writes the path as an absolute XPath 3.0 path expression that
/// selects the item, every name in the braced-URI form, e.g.
/// `/Q{urn:po}order[1]/Q{}item[3]/@Q{}qty`. The first step is the
/// validation root: for a run over a fragment pushed through the API, the
/// path starts at that fragment's outermost element.
///
/// Paths share their ancestor steps: the diagnostics of one run hold one copy
/// of each ancestor, so reporting an error costs the same at any depth. The
/// representation is private.
#[derive(Clone, Default)]
pub struct InstancePath {
    last: Option<Arc<PathNode>>,
}

impl InstancePath {
    /// A path of `steps`, outermost first.
    pub fn new(steps: Vec<PathStep>) -> Self {
        steps
            .into_iter()
            .fold(InstancePath::default(), |path, step| path.child(step))
    }

    /// This path extended by `step` (sharing this path).
    pub(crate) fn child(&self, step: PathStep) -> Self {
        InstancePath {
            last: Some(Arc::new(PathNode {
                step,
                parent: self.last.clone(),
                len: self.len() + 1,
            })),
        }
    }

    /// The number of steps.
    pub fn len(&self) -> usize {
        self.last.as_ref().map_or(0, |node| node.len)
    }

    /// Whether the path has no step (the document).
    pub fn is_empty(&self) -> bool {
        self.last.is_none()
    }

    /// The last step: the item the path designates.
    pub fn last(&self) -> Option<&PathStep> {
        self.last.as_deref().map(|node| &node.step)
    }

    /// The steps, outermost first: a double-ended iterator that knows its
    /// length (`.rev()` gives them innermost first).
    ///
    /// The path is stored innermost first (each step links to its parent), so
    /// the iterator gathers the step references on creation: in a buffer on
    /// the stack for a path of up to 32 steps — no allocation — and in one
    /// `Vec` for a deeper path.
    pub fn steps(&self) -> impl DoubleEndedIterator<Item = &PathStep> + ExactSizeIterator + '_ {
        PathSteps::new(self)
    }

    /// The nodes, innermost first.
    fn nodes(&self) -> impl Iterator<Item = &PathNode> {
        std::iter::successors(self.last.as_deref(), |node| node.parent.as_deref())
    }

    /// The local names of the element steps, `/root/child` — the form of
    /// [`ValidationError::element_path`].
    pub(crate) fn local_name_path(&self) -> String {
        let mut names: Vec<&str> = self
            .nodes()
            .filter(|node| matches!(node.step, PathStep::Element { .. }))
            .map(|node| node.step.name().local_name())
            .collect();
        names.reverse();
        let mut path = String::with_capacity(names.iter().map(|n| n.len() + 1).sum());
        for name in names {
            path.push('/');
            path.push_str(name);
        }
        path
    }
}

/// Steps a [`PathSteps`] holds on the stack before it allocates.
const INLINE_STEPS: usize = 32;

/// The iterator of [`InstancePath::steps`]: the step references, outermost
/// first, at `front..back` of `inline` (a path of up to [`INLINE_STEPS`]
/// steps) or of `heap` (a deeper one; empty, so unallocated, otherwise).
struct PathSteps<'a> {
    inline: [Option<&'a PathStep>; INLINE_STEPS],
    heap: Vec<&'a PathStep>,
    front: usize,
    back: usize,
}

impl<'a> PathSteps<'a> {
    fn new(path: &'a InstancePath) -> Self {
        let len = path.len();
        let mut inline = [None; INLINE_STEPS];
        let mut heap = Vec::new();
        if len <= INLINE_STEPS {
            for (slot, node) in inline[..len].iter_mut().rev().zip(path.nodes()) {
                *slot = Some(&node.step);
            }
        } else {
            heap = path.nodes().map(|node| &node.step).collect();
            heap.reverse();
        }
        PathSteps {
            inline,
            heap,
            front: 0,
            back: len,
        }
    }

    fn get(&self, i: usize) -> &'a PathStep {
        if self.heap.is_empty() {
            self.inline[i].expect("a filled step")
        } else {
            self.heap[i]
        }
    }
}

impl<'a> Iterator for PathSteps<'a> {
    type Item = &'a PathStep;

    fn next(&mut self) -> Option<&'a PathStep> {
        if self.front == self.back {
            return None;
        }
        self.front += 1;
        Some(self.get(self.front - 1))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
}

impl DoubleEndedIterator for PathSteps<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.get(self.back))
    }
}

impl ExactSizeIterator for PathSteps<'_> {}

impl std::iter::FusedIterator for PathSteps<'_> {}

impl PartialEq for InstancePath {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let mut a = self.last.as_ref();
        let mut b = other.last.as_ref();
        while let (Some(x), Some(y)) = (a, b) {
            if Arc::ptr_eq(x, y) {
                return true; // the rest is shared
            }
            if x.step != y.step {
                return false;
            }
            a = x.parent.as_ref();
            b = y.parent.as_ref();
        }
        true
    }
}

impl Eq for InstancePath {}

impl std::hash::Hash for InstancePath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self.len());
        for node in self.nodes() {
            node.step.hash(state);
        }
    }
}

impl std::fmt::Debug for InstancePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InstancePath")
            .field(&format_args!("{self}"))
            .finish()
    }
}

impl From<Vec<PathStep>> for InstancePath {
    fn from(steps: Vec<PathStep>) -> Self {
        InstancePath::new(steps)
    }
}

impl std::fmt::Display for InstancePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            return f.write_str("/");
        }
        for step in self.steps() {
            write!(f, "/{step}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ValidationError
// ---------------------------------------------------------------------------

/// The subject and the path of a diagnostic, held behind one `Box` so that
/// carrying them costs a [`ValidationError`] one pointer: the error travels
/// in `Result`s on the hot path of simple-type validation, where it is built
/// without either.
#[derive(Debug, Clone, Default)]
pub(crate) struct DiagnosticItem {
    /// Optional owned evidence, allocated only for diagnostics that have it.
    evidence: Option<Box<DiagnosticEvidence>>,
    subject: Option<ValidationSubject>,
    instance_path: Option<InstancePath>,
    /// Set when the local-name element path is the element part of
    /// `instance_path`: it is then written only when first asked for.
    derived_element_path: Option<OnceLock<String>>,
}

impl DiagnosticItem {
    pub(crate) fn subject(item: &Option<Box<Self>>) -> Option<&ValidationSubject> {
        item.as_ref().and_then(|i| i.subject.as_ref())
    }

    pub(crate) fn instance_path(item: &Option<Box<Self>>) -> Option<&InstancePath> {
        item.as_ref().and_then(|i| i.instance_path.as_ref())
    }

    pub(crate) fn set_subject(item: &mut Option<Box<Self>>, subject: ValidationSubject) {
        item.get_or_insert_with(Default::default).subject = Some(subject);
    }

    pub(crate) fn set_instance_path(item: &mut Option<Box<Self>>, path: InstancePath) {
        item.get_or_insert_with(Default::default).instance_path = Some(path);
    }
}

/// Instance validation error with spec-aligned constraint code.
///
/// Besides the code and the message, an error reported by the validation
/// runtime names its [`subject`](Self::subject) — the element or attribute
/// it is about — and the [`instance_path`](Self::instance_path) to it, so a
/// host can filter errors or build a fault report without parsing the
/// message. Both are `None` when the diagnostic is not about an instance
/// item (a schema-level problem, or a run-level error such as an unclosed
/// element at the end).
///
/// The struct is `#[non_exhaustive]`: build one with [`error`],
/// [`error_with_path`], [`from_value_error`] or [`from_facet_error`], then
/// set the public fields or use the `with_*` builders.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ValidationError {
    /// Spec constraint code (cvc-*, cos-*, src-*)
    pub constraint: &'static str,
    /// Human-readable error message
    pub message: String,
    /// Source location in the instance document
    pub location: Option<SourceLocation>,
    /// The local-name path set by [`with_path`](Self::with_path); see
    /// [`element_path`](Self::element_path).
    element_path: Option<String>,
    /// Subject and positional path; see [`subject`](Self::subject).
    item: Option<Box<DiagnosticItem>>,
}

impl std::fmt::Display for ValidationError {
    /// `[constraint] message`, then ` at location` and ` (element path)` when
    /// they are known. The element path is the local-name form of
    /// [`element_path`](Self::element_path); the structured
    /// [`instance_path`](Self::instance_path) is not written.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.constraint, self.message)?;
        if let Some(loc) = &self.location {
            write!(f, " at {}", loc)?;
        }
        if let Some(path) = self.element_path() {
            write!(f, " ({})", path)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

impl ValidationError {
    /// Owned native evidence, when captured at the failing branch. `None` means
    /// unavailable; neither codes nor messages are parsed to reconstruct it.
    pub fn evidence(&self) -> Option<&DiagnosticEvidence> {
        self.item
            .as_deref()
            .and_then(|item| item.evidence.as_deref())
    }

    fn evidence_mut(&mut self) -> &mut DiagnosticEvidence {
        self.item
            .get_or_insert_with(Default::default)
            .evidence
            .get_or_insert_with(Default::default)
    }

    /// Attach a native cause without changing the raw diagnostic fields.
    pub fn with_native_cause(mut self, cause: NativeCause) -> Self {
        self.evidence_mut().cause = Some(cause);
        self
    }

    /// Record a governing type and the innermost observed validation dispatch.
    /// This source identifies the resolved type, not the declaration of an
    /// inherited effective facet (which remains unavailable).
    pub(crate) fn with_type_context(
        mut self,
        key: crate::ids::TypeKey,
        schema: &crate::schema::SchemaSet,
    ) -> Self {
        let component = SchemaComponent::for_type(key, schema);
        let evidence = self.evidence_mut();
        if evidence.actual_type.is_none() {
            evidence.actual_type = component.clone();
        }
        evidence.governing_type = component;
        self
    }

    pub(crate) fn with_xsi_types(
        mut self,
        governing: crate::ids::TypeKey,
        actual: crate::ids::TypeKey,
        schema: &crate::schema::SchemaSet,
    ) -> Self {
        let evidence = self.evidence_mut();
        evidence.governing_type = SchemaComponent::for_type(governing, schema);
        evidence.actual_type = SchemaComponent::for_type(actual, schema);
        self
    }

    pub(crate) fn with_component(mut self, component: Option<SchemaComponent>) -> Self {
        self.evidence_mut().component = component;
        self
    }

    /// Set the source location.
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the local-name [`element_path`](Self::element_path).
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.element_path = Some(path.into());
        self
    }

    /// Set the [`subject`](Self::subject).
    pub fn with_subject(mut self, subject: ValidationSubject) -> Self {
        DiagnosticItem::set_subject(&mut self.item, subject);
        self
    }

    /// Set the [`instance_path`](Self::instance_path).
    pub fn with_instance_path(mut self, path: InstancePath) -> Self {
        DiagnosticItem::set_instance_path(&mut self.item, path);
        self
    }

    /// The element or attribute the error is about, when there is one.
    pub fn subject(&self) -> Option<&ValidationSubject> {
        DiagnosticItem::subject(&self.item)
    }

    /// The namespace-aware positional path to the [`subject`](Self::subject)
    /// — `/Q{uri}local[n]/…`, ending in `@Q{uri}local` for an attribute.
    pub fn instance_path(&self) -> Option<&InstancePath> {
        DiagnosticItem::instance_path(&self.item)
    }

    /// The path to the element as local names, `/root/child` — no
    /// namespaces, no positions; for an attribute error, the path of its
    /// element. Kept as a readable convenience (it is what `Display` writes);
    /// [`instance_path`](Self::instance_path) identifies the item exactly.
    pub fn element_path(&self) -> Option<&str> {
        if let Some(path) = &self.element_path {
            return Some(path);
        }
        let item = self.item.as_deref()?;
        let cell = item.derived_element_path.as_ref()?;
        let path = item.instance_path.as_ref()?;
        Some(cell.get_or_init(|| path.local_name_path()))
    }

    /// Make the local-name element path the element part of the
    /// [`instance_path`](Self::instance_path), written only when first asked
    /// for (replacing a path set by [`with_path`](Self::with_path)). No
    /// effect without an instance path.
    pub(crate) fn derive_element_path(&mut self) {
        if let Some(item) = self.item.as_deref_mut() {
            if item.instance_path.is_some() {
                item.derived_element_path = Some(OnceLock::new());
                self.element_path = None;
            }
        }
    }
}

/// Result type for instance validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Create a validation error with the given constraint code and message
pub fn error(
    constraint: &'static str,
    message: impl Into<String>,
    location: Option<SourceLocation>,
) -> ValidationError {
    ValidationError {
        constraint,
        message: message.into(),
        location,
        element_path: None,
        item: None,
    }
}

/// Create a validation error with element path information
pub fn error_with_path(
    constraint: &'static str,
    message: impl Into<String>,
    location: Option<SourceLocation>,
    element_path: impl Into<String>,
) -> ValidationError {
    ValidationError {
        constraint,
        message: message.into(),
        location,
        element_path: Some(element_path.into()),
        item: None,
    }
}

/// Convert a type validation error to an instance validation error
///
/// Use this when a `types::validators::ValidationError` needs to be reported
/// with a specific cvc-* constraint code.
pub fn from_value_error(
    constraint: &'static str,
    err: TypeValidationError,
    location: Option<SourceLocation>,
) -> ValidationError {
    ValidationError {
        constraint,
        message: err.to_string(),
        location,
        element_path: None,
        item: None,
    }
    .with_native_cause(NativeCause::Value(err))
}

/// Convert a type validation error using its default cvc-* constraint code.
///
/// Convenience over [`from_value_error`] for callers that don't override the
/// code: it derives the constraint via [`value_error_constraint_code`] and
/// builds the [`ValidationError`].
pub fn from_value_error_default(
    err: TypeValidationError,
    location: Option<SourceLocation>,
) -> ValidationError {
    let constraint = value_error_constraint_code(&err);
    from_value_error(constraint, err, location)
}

/// Convert a facet error to an instance validation error
///
/// Use this when a `FacetError` needs to be reported with a specific cvc-* code.
/// Consider using `facet_constraint_code()` to get the appropriate code.
pub fn from_facet_error(
    constraint: &'static str,
    err: FacetError,
    location: Option<SourceLocation>,
) -> ValidationError {
    ValidationError {
        constraint,
        message: err.to_string(),
        location,
        element_path: None,
        item: None,
    }
    .with_native_cause(NativeCause::Facet(err))
}

/// Map a FacetError variant to its specific cvc-* constraint code
///
/// This function returns the most specific constraint code for each facet type,
/// preferring codes like `cvc-pattern-valid` over generic `cvc-facet-valid`.
///
/// # Mappings
///
/// | FacetError Variant | Constraint Code |
/// |--------------------|-----------------|
/// | LengthViolation | cvc-length-valid |
/// | MinLengthViolation | cvc-minLength-valid |
/// | MaxLengthViolation | cvc-maxLength-valid |
/// | PatternViolation | cvc-pattern-valid |
/// | EnumerationViolation | cvc-enumeration-valid |
/// | MinInclusiveViolation | cvc-minInclusive-valid |
/// | MaxInclusiveViolation | cvc-maxInclusive-valid |
/// | MinExclusiveViolation | cvc-minExclusive-valid |
/// | MaxExclusiveViolation | cvc-maxExclusive-valid |
/// | TotalDigitsViolation | cvc-totalDigits-valid |
/// | FractionDigitsViolation | cvc-fractionDigits-valid |
/// | ExplicitTimezoneViolation | cvc-explicitTimezone-valid |
/// | InvalidPattern | cvc-pattern-valid |
/// | DerivationRestriction | cos-st-restricts |
/// | FixedFacetViolation | cos-st-restricts |
/// | ConflictingFacets | cos-st-restricts |
/// | NotApplicable | cos-applicable-facets |
pub fn facet_constraint_code(err: &FacetError) -> &'static str {
    match err {
        FacetError::LengthViolation { .. } => "cvc-length-valid",
        FacetError::MinLengthViolation { .. } => "cvc-minLength-valid",
        FacetError::MaxLengthViolation { .. } => "cvc-maxLength-valid",
        FacetError::PatternViolation { .. } => "cvc-pattern-valid",
        FacetError::EnumerationViolation { .. } => "cvc-enumeration-valid",
        FacetError::MinInclusiveViolation { .. } => "cvc-minInclusive-valid",
        FacetError::MaxInclusiveViolation { .. } => "cvc-maxInclusive-valid",
        FacetError::MinExclusiveViolation { .. } => "cvc-minExclusive-valid",
        FacetError::MaxExclusiveViolation { .. } => "cvc-maxExclusive-valid",
        FacetError::TotalDigitsViolation { .. } => "cvc-totalDigits-valid",
        FacetError::FractionDigitsViolation { .. } => "cvc-fractionDigits-valid",
        FacetError::ExplicitTimezoneViolation { .. } => "cvc-explicitTimezone-valid",
        FacetError::InvalidPattern { .. } => "cvc-pattern-valid",
        FacetError::DerivationRestriction { .. } => "cos-st-restricts",
        FacetError::FixedFacetViolation { .. } => "cos-st-restricts",
        FacetError::ConflictingFacets { .. } => "cos-st-restricts",
        FacetError::NotApplicable { .. } => "cos-applicable-facets",
    }
}

/// Map a TypeValidationError variant to its default cvc-* constraint code
///
/// Returns `cvc-datatype-valid` for lexical/type/range errors, and delegates
/// to `facet_constraint_code()` for facet violations.
///
/// # Constraint code context
///
/// - `cvc-datatype-valid` (datatypes.html) is the default for lexical/type/range
///   errors. Callers validating at the structures.html level (e.g., element or
///   attribute value checks) should override to `cvc-simple-type` instead.
/// - `FacetViolation` delegates to `facet_constraint_code()`, which returns
///   `cvc-*-valid` for runtime facet checks. Schema-level `FacetError` variants
///   (`DerivationRestriction`, `FixedFacetViolation`, `ConflictingFacets`,
///   `NotApplicable`) map to `cos-*` codes and should not appear during instance
///   validation — they are schema compilation errors.
pub fn value_error_constraint_code(err: &TypeValidationError) -> &'static str {
    match err {
        TypeValidationError::InvalidLexical { .. } => "cvc-datatype-valid",
        TypeValidationError::FacetViolation(facet_err) => facet_constraint_code(facet_err),
        TypeValidationError::TypeError { .. } => "cvc-datatype-valid",
        TypeValidationError::RangeError { .. } => "cvc-datatype-valid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_constructor() {
        let err = error("cvc-elt", "Element is invalid", None);
        assert_eq!(err.constraint, "cvc-elt");
        assert_eq!(err.message, "Element is invalid");
        assert!(err.location.is_none());
        assert!(err.element_path().is_none());
        assert!(err.subject().is_none());
        assert!(err.instance_path().is_none());
    }

    #[test]
    fn test_error_with_path() {
        let err = error_with_path(
            "cvc-complex-type",
            "Missing required element",
            None,
            "/root/child",
        );
        assert_eq!(err.constraint, "cvc-complex-type");
        assert_eq!(err.element_path(), Some("/root/child"));
    }

    #[test]
    fn test_error_display() {
        let err = error("cvc-elt", "Invalid element", None);
        assert_eq!(format!("{}", err), "[cvc-elt] Invalid element");

        let err_with_path = error_with_path("cvc-type", "Type mismatch", None, "/root");
        assert_eq!(
            format!("{}", err_with_path),
            "[cvc-type] Type mismatch (/root)"
        );
    }

    #[test]
    fn test_facet_constraint_code_mapping() {
        // Test all facet error variants map to correct codes
        assert_eq!(
            facet_constraint_code(&FacetError::LengthViolation {
                message: "test".to_string()
            }),
            "cvc-length-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::MinLengthViolation { actual: 1, min: 5 }),
            "cvc-minLength-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::MaxLengthViolation { actual: 10, max: 5 }),
            "cvc-maxLength-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::PatternViolation {
                value: "abc".to_string(),
                pattern: "[0-9]+".to_string()
            }),
            "cvc-pattern-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::EnumerationViolation {
                value: "x".to_string()
            }),
            "cvc-enumeration-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::MinInclusiveViolation {
                value: "1".to_string(),
                min: "5".to_string()
            }),
            "cvc-minInclusive-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::MaxInclusiveViolation {
                value: "10".to_string(),
                max: "5".to_string()
            }),
            "cvc-maxInclusive-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::MinExclusiveViolation {
                value: "5".to_string(),
                min: "5".to_string()
            }),
            "cvc-minExclusive-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::MaxExclusiveViolation {
                value: "5".to_string(),
                max: "5".to_string()
            }),
            "cvc-maxExclusive-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::TotalDigitsViolation { actual: 10, max: 5 }),
            "cvc-totalDigits-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::FractionDigitsViolation { actual: 5, max: 2 }),
            "cvc-fractionDigits-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::ExplicitTimezoneViolation {
                message: "test".to_string()
            }),
            "cvc-explicitTimezone-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::InvalidPattern {
                pattern: "[".to_string(),
                message: "invalid".to_string()
            }),
            "cvc-pattern-valid"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::DerivationRestriction {
                message: "test".to_string()
            }),
            "cos-st-restricts"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::FixedFacetViolation {
                facet_name: "length".to_string(),
                base_value: "5".to_string(),
                derived_value: "10".to_string()
            }),
            "cos-st-restricts"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::ConflictingFacets {
                message: "test".to_string()
            }),
            "cos-st-restricts"
        );
        assert_eq!(
            facet_constraint_code(&FacetError::NotApplicable {
                facet: "length".to_string(),
                type_name: "integer".to_string()
            }),
            "cos-applicable-facets"
        );
    }

    #[test]
    fn test_from_facet_error() {
        let facet_err = FacetError::MinLengthViolation { actual: 2, min: 5 };
        let code = facet_constraint_code(&facet_err);
        let val_err = from_facet_error(code, facet_err, None);
        assert_eq!(val_err.constraint, "cvc-minLength-valid");
        assert!(val_err.message.contains("minLength"));
    }

    #[test]
    fn test_value_error_constraint_code_invalid_lexical() {
        let err = TypeValidationError::InvalidLexical {
            value: "abc".to_string(),
            type_name: "integer",
            message: "not a valid integer".to_string(),
        };
        assert_eq!(value_error_constraint_code(&err), "cvc-datatype-valid");
    }

    #[test]
    fn test_value_error_constraint_code_facet_violation() {
        let err = TypeValidationError::FacetViolation(FacetError::PatternViolation {
            value: "abc".to_string(),
            pattern: "[0-9]+".to_string(),
        });
        assert_eq!(value_error_constraint_code(&err), "cvc-pattern-valid");
    }

    #[test]
    fn test_value_error_constraint_code_type_error() {
        use crate::types::XmlTypeCode;
        let err = TypeValidationError::TypeError {
            expected: XmlTypeCode::String,
            actual: XmlTypeCode::Integer,
        };
        assert_eq!(value_error_constraint_code(&err), "cvc-datatype-valid");
    }

    #[test]
    fn test_value_error_constraint_code_range_error() {
        let err = TypeValidationError::RangeError {
            value: "999999".to_string(),
            type_name: "short",
        };
        assert_eq!(value_error_constraint_code(&err), "cvc-datatype-valid");
    }

    #[test]
    fn test_with_location() {
        let loc = SourceLocation {
            base_uri: "test.xsd".to_string(),
            line: 10,
            column: 5,
        };
        let err = error("cvc-elt", "test", None).with_location(loc.clone());
        assert_eq!(err.location, Some(loc));
    }

    #[test]
    fn test_with_path() {
        let err = error("cvc-elt", "test", None).with_path("/root/child");
        assert_eq!(err.element_path(), Some("/root/child"));
    }

    #[test]
    fn test_builder_chaining() {
        let loc = SourceLocation {
            base_uri: "test.xsd".to_string(),
            line: 3,
            column: 1,
        };
        let err = error("cvc-type", "Type mismatch", None)
            .with_location(loc)
            .with_path("/root/elem[2]");
        assert_eq!(err.constraint, "cvc-type");
        assert_eq!(err.location.as_ref().unwrap().line, 3);
        assert_eq!(err.element_path(), Some("/root/elem[2]"));
        let display = format!("{}", err);
        assert!(display.contains("[cvc-type]"));
        assert!(display.contains("Type mismatch"));
        assert!(display.contains("/root/elem[2]"));
    }

    #[test]
    fn test_from_value_error_with_auto_code() {
        let type_err = TypeValidationError::InvalidLexical {
            value: "not-a-number".to_string(),
            type_name: "decimal",
            message: "invalid decimal".to_string(),
        };
        let code = value_error_constraint_code(&type_err);
        let val_err = from_value_error(code, type_err, None);
        assert_eq!(val_err.constraint, "cvc-datatype-valid");
        assert!(val_err.message.contains("invalid decimal"));
    }
}

/// Evidence copied while the backend's typed state is available. All members
/// are owned; no arena keys, document IDs or worker-local name IDs escape.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticEvidence {
    /// Cause captured at the emission site, or unavailable.
    pub cause: Option<NativeCause>,
    /// Outermost type governing the failed validation operation.
    pub governing_type: Option<SchemaComponent>,
    /// Innermost type dispatch observed on the failing path. For atomic types
    /// this can equal the governing type even when a primitive/facet is inherited.
    pub actual_type: Option<SchemaComponent>,
    /// Specific declaration involved, when the branch has one.
    pub component: Option<SchemaComponent>,
    /// Exact effective-facet origin; currently unavailable. A type source must
    /// never be mistaken for the ancestor that supplied an effective facet.
    pub facet_origin: Option<SchemaComponent>,
}

/// Component kind recorded from a resolved schema component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    /// Simple type definition.
    SimpleType,
    /// Complex type definition.
    ComplexType,
    /// Attribute declaration.
    Attribute,
    /// Element declaration.
    Element,
}

/// Owned schema origin, translated from `SourceRef`, never an invoice span.
/// A local name is descriptive, not a globally unique component identity.
#[derive(Debug, Clone)]
pub struct SchemaComponent {
    /// Actual component kind.
    pub kind: ComponentKind,
    /// Actual declared name; `None` for anonymous types.
    pub name: Option<ExpandedName>,
    /// Whether a declaration is global; not applicable to type definitions.
    pub global: Option<bool>,
    /// Schema document URI, if its recorded source document is available.
    pub document_uri: Option<String>,
    /// Recorded schema span (often only the start tag), not a declaration range.
    pub schema_span: Option<(usize, usize)>,
}

impl SchemaComponent {
    fn new(
        kind: ComponentKind,
        name: Option<ExpandedName>,
        global: Option<bool>,
        source: Option<&crate::parser::location::SourceRef>,
        schema: &crate::schema::SchemaSet,
    ) -> Self {
        Self {
            kind,
            name,
            global,
            document_uri: source.and_then(|s| {
                schema
                    .documents
                    .iter()
                    .find(|d| d.id == s.doc_id)
                    .map(|d| d.base_uri.clone())
            }),
            schema_span: source.map(|s| (s.span.start, s.span.end)),
        }
    }

    pub(crate) fn for_type(
        key: crate::ids::TypeKey,
        schema: &crate::schema::SchemaSet,
    ) -> Option<Self> {
        let (kind, local, ns, source) = match key {
            crate::ids::TypeKey::Simple(k) => {
                let t = schema.arenas.simple_types.get(k)?;
                (
                    ComponentKind::SimpleType,
                    t.name,
                    t.target_namespace,
                    t.source.as_ref(),
                )
            }
            crate::ids::TypeKey::Complex(k) => {
                let t = schema.arenas.complex_types.get(k)?;
                (
                    ComponentKind::ComplexType,
                    t.name,
                    t.target_namespace,
                    t.source.as_ref(),
                )
            }
        };
        let names = &schema.name_table;
        let name = local.map(|local| {
            ExpandedName::new(ns.map(|n| names.resolve_ref(n)), names.resolve_ref(local))
        });
        Some(Self::new(kind, name, None, source, schema))
    }

    pub(crate) fn for_attribute(
        key: crate::ids::AttributeKey,
        schema: &crate::schema::SchemaSet,
    ) -> Option<Self> {
        let d = schema.arenas.attributes.get(key)?;
        let names = &schema.name_table;
        Some(Self::new(
            ComponentKind::Attribute,
            d.name.map(|name| {
                ExpandedName::new(
                    d.target_namespace.map(|n| names.resolve_ref(n)),
                    names.resolve_ref(name),
                )
            }),
            d.name
                .map(|name| schema.lookup_attribute(d.target_namespace, name) == Some(key)),
            d.source.as_ref(),
            schema,
        ))
    }

    pub(crate) fn for_element(
        key: crate::ids::ElementKey,
        schema: &crate::schema::SchemaSet,
    ) -> Option<Self> {
        let d = schema.arenas.elements.get(key)?;
        let names = &schema.name_table;
        Some(Self::new(
            ComponentKind::Element,
            d.name.map(|name| {
                ExpandedName::new(
                    d.target_namespace.map(|n| names.resolve_ref(n)),
                    names.resolve_ref(name),
                )
            }),
            d.name
                .map(|name| schema.lookup_element(d.target_namespace, name) == Some(key)),
            d.source.as_ref(),
            schema,
        ))
    }
}

/// Native failure causes. These are backend observations, not reference-engine
/// subcodes or a complete classification of all schema diagnostics.
#[derive(Debug, Clone)]
pub enum NativeCause {
    /// Original typed value-validation error, including its facet payload.
    Value(TypeValidationError),
    /// Original facet error.
    Facet(FacetError),
    /// Parent content model rejected this expanded child name.
    UnexpectedChild {
        /// Triggering child; raw diagnostic still names its parent.
        found: ExpandedName,
    },
    /// Popped element's content model could not finish.
    IncompleteContent,
    /// Required attribute absent on the existing owner element.
    MissingRequiredAttribute {
        /// Actual required expanded name.
        name: ExpandedName,
    },
    /// Prohibited attribute (no exact use-component captured).
    ProhibitedAttribute,
    /// No allowed attribute use or effective wildcard matched.
    AttributeNotAllowed,
    /// Attribute fixed-value comparison failed.
    AttributeFixedValue {
        /// Actual fixed constraint.
        expected: String,
        /// Actual instance lexical value.
        actual: String,
    },
    /// No element declaration found under strict assessment.
    UndeclaredElement,
    /// `xsi:nil` present on a non-nillable declaration.
    NilNotAllowed,
    /// Nilled element also has a fixed constraint.
    NilWithFixedValue,
    /// Nilled element has prohibited child content.
    NilWithContent,
    /// `xsi:type` QName could not be parsed/resolved.
    XsiTypeUnresolved {
        /// Original lexical value.
        value: String,
    },
    /// Resolved `xsi:type` failed derivation or substitution checks.
    XsiTypeDerivation {
        /// Original lexical value.
        value: String,
        /// True for substitution blocked by `block`.
        blocked: bool,
    },
}
