//! XPath tree comparison helpers.
//!
//! Port of `xpath2/XPath20Api/XPath20Api/TreeComparer.cs`.
//! Aligns with `DOM_NAVIGATOR_DESIGN.md` and `XML_NODE_ITERATOR_DESIGN.md`.

use crate::ids::{ComplexTypeKey, TypeKey};
use crate::navigator::TypedValue;
use crate::schema::SchemaSet;
use crate::types::XmlTypeCode;
use crate::types::{normalize_whitespace, WhitespaceMode, XmlAtomicValue, XmlValue, XmlValueKind};
use crate::validation::info::ContentType;
use crate::validation::runtime::determine_content_type;

use super::ast::BinaryOpKind;
use super::atomize::{self, Atoms};
use super::collation::CollationRef;
use super::error::XPathError;
use super::iterator::{XmlItemRef, XmlNodeIterator};
use super::operators::{eval_binary_collated, is_gregorian_code, is_temporal_type};
use super::string_ops::is_xml_whitespace;
use super::{DomNavigator, DomNodeType};
use crate::types::value::TimezoneOffset;

/// Compares XPath nodes and sequences for deep equality.
#[derive(Debug, Clone, Default)]
pub struct TreeComparer {
    pub ignore_whitespace: bool,
}

impl TreeComparer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_ignore_whitespace(ignore_whitespace: bool) -> Self {
        Self { ignore_whitespace }
    }

    /// Deep equality of the **children** of two navigator positions.
    ///
    /// This compares the two child sequences pairwise; it deliberately does
    /// *not* look at the two positions themselves, so their names, attributes
    /// and node kinds play no part. On two document nodes that is exactly
    /// `fn:deep-equal`, whose content is its children; on two elements it is
    /// a *content* comparison — `<a x="1">t</a>` and `<b y="2">t</b>` are
    /// reported equal here, while `fn:deep-equal` reports them different.
    ///
    /// Every child is compared, comment and processing-instruction children
    /// included — which is *stricter* than `fn:deep-equal`, whose content
    /// model leaves those out. The strict reading is what a serialization
    /// round-trip check wants, and it is the published behaviour of this type;
    /// `fn:deep-equal` uses the crate-private comparer instead.
    ///
    /// To compare items the way `fn:deep-equal` does, including the node kind
    /// and name, use [`deep_equal_iter`](Self::deep_equal_iter) over the two
    /// sequences.
    ///
    /// The comparison walks the two trees iteratively, so they may be of any
    /// depth whatever the thread's stack; it stops at the first difference.
    pub fn deep_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        NodeComparer::legacy(self.ignore_whitespace).deep_equal(left, right)
    }

    /// Deep equality for two XPath item iterators.
    ///
    /// Node items are compared by kind, name and content; atomic items with
    /// `eq` semantics, NaN equal to NaN, and a pair `eq` is not defined for
    /// reported unequal rather than raising. Element content is compared the
    /// same way [`deep_equal`](Self::deep_equal) compares it, comments and PIs
    /// included.
    pub fn deep_equal_iter<I>(&self, left: &I, right: &I) -> Result<bool, XPathError>
    where
        I: XmlNodeIterator,
    {
        NodeComparer::legacy(self.ignore_whitespace).deep_equal_iter(left, right)
    }
}

/// Which of the four content cases of the element rule an element falls into.
///
/// The rule requires the two elements to be in the *same* case; the case then
/// decides what is compared. `Simple` is "annotated as having simple content"
/// and the other two are "annotated as having complex content", which is the
/// distinction the rule's clause (2) turns on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementContent {
    /// Clause 4(a): the element has a simple type annotation, or a complex
    /// type whose content is text-only. Its *typed value* is compared.
    Simple,
    /// Clauses 4(b) and 4(d): complex content that is element-only, or empty.
    /// Only the child **elements** are compared; text children are not looked
    /// at. The two clauses are merged because they prescribe the same
    /// comparison — an empty content type admits no child elements, so 4(d) is
    /// 4(b) with both child-element sequences empty.
    ElementOnly,
    /// Clause 4(c): complex content that is mixed. The `(*|text())` sequences
    /// are compared. An element with no type annotation is `xs:untyped`, which
    /// is complex content and mixed, so every node of an unvalidated document
    /// lands here.
    Mixed,
    /// The case cannot be told: a nilled element of a complex type, with no
    /// schema set to read the type's content kind from. Its typed value is the
    /// empty sequence whatever that kind is, so the typed value does not tell
    /// simple content from complex content either. It is not a case of the
    /// rule; [`NodeComparer::element_content_step`] resolves it against the
    /// other element.
    Unknown,
}

/// The comparison engine behind [`TreeComparer`] and `fn:deep-equal`.
///
/// It is deliberately **crate-private**. [`TreeComparer`] is published with a
/// single public field, so it can be built by struct literal out of crate and
/// gaining a field would be a breaking change; the extra state the
/// `fn:deep-equal` rules need lives here instead, and `TreeComparer`'s methods
/// delegate with that state switched off. The published behaviour is therefore
/// exactly what it was.
///
/// `function_rules` selects between the two:
///
/// * `false` — the historical comparison. Every child is compared, including
///   comment and processing-instruction children, and an attribute's typed
///   values are compared with plain value equality. This is what the XQTS
///   judge and the copy round-trip check need, and what [`TreeComparer`] does.
/// * `true` — the `fn:deep-equal` rules (F&O §15.3.1): comment and PI children
///   play no part, an element is compared according to its content case, and
///   typed values are compared with `eq` semantics.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeComparer<'s> {
    ignore_whitespace: bool,
    function_rules: bool,
    schema_set: Option<&'s SchemaSet>,
    /// The collation every string comparison of the `fn:deep-equal` rules uses
    /// — text, comments, processing-instruction contents, typed values and
    /// free-standing atomic items — but never a *name*. [`TreeComparer`] is the
    /// codepoint collation, which is what a round-trip check wants and what it
    /// has always done.
    collation: CollationRef<'s>,
    /// The implicit timezone of the dynamic context, for date/time values
    /// without one (F&O §10.4); the host's offset for [`TreeComparer`].
    implicit_timezone: TimezoneOffset,
}

impl NodeComparer<'static> {
    /// The comparison [`TreeComparer`] performs.
    fn legacy(ignore_whitespace: bool) -> Self {
        Self {
            ignore_whitespace,
            function_rules: false,
            schema_set: None,
            collation: CollationRef::Codepoint,
            implicit_timezone: super::operators::host_implicit_timezone(),
        }
    }
}

impl<'s> NodeComparer<'s> {
    /// The comparison `fn:deep-equal` performs (F&O §15.3.1).
    ///
    /// `schema_set` is the static context's schema set, used to read a complex
    /// type's content kind — element-only, mixed or empty — which decides
    /// which of the rule's clause-4 cases applies. With `None` (the usual case:
    /// no schema was imported) an element's typed value decides between simple
    /// and complex content, and complex content is compared as mixed, which is
    /// the unvalidated behaviour; see
    /// [`element_content`](Self::element_content) for the details, nilled
    /// elements included.
    ///
    /// `ignore_whitespace` is `false`: `fn:deep-equal` compares text nodes
    /// exactly, and the option exists for test harnesses, not for the function.
    pub(crate) fn deep_equal_function(
        schema_set: Option<&'s SchemaSet>,
        collation: CollationRef<'s>,
        implicit_timezone: TimezoneOffset,
    ) -> Self {
        Self {
            ignore_whitespace: false,
            function_rules: true,
            schema_set,
            collation,
            implicit_timezone,
        }
    }

    /// String equality under this comparer's collation.
    ///
    /// [`CollationRef::equals`] can only fail for
    /// [`CollationRef::Unsupported`], which cannot reach here: `fn:deep-equal`
    /// resolves its collation with
    /// [`require`](crate::xpath::collation::ActiveCollation::require), so an
    /// unsupported URI has already raised FOCH0002 before any node is looked
    /// at, and [`TreeComparer`] is always the codepoint collation.
    #[inline]
    fn collated_equal(&self, left: &str, right: &str) -> bool {
        self.collation.equals(left, right).unwrap_or(false)
    }

    fn text_equal(&self, left: &str, right: &str) -> bool {
        if self.ignore_whitespace {
            self.collated_equal(
                &normalize_whitespace(left, WhitespaceMode::Collapse),
                &normalize_whitespace(right, WhitespaceMode::Collapse),
            )
        } else {
            self.collated_equal(left, right)
        }
    }

    fn normalized_item_value(&self, value: &XmlValue) -> XmlValue {
        match value.type_code {
            XmlTypeCode::UntypedAtomic | XmlTypeCode::AnyUri => {
                XmlValue::string(value.to_string_value())
            }
            _ => value.clone(),
        }
    }

    fn is_nan_value(&self, value: &XmlValue) -> bool {
        match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Float(f)) => f.is_nan(),
            XmlValueKind::Atomic(XmlAtomicValue::Double(d)) => d.is_nan(),
            _ => false,
        }
    }

    fn values_equal_or_nan(&self, left: &XmlValue, right: &XmlValue) -> bool {
        if self.is_nan_value(left) && self.is_nan_value(right) {
            return true;
        }
        // Date/time, duration and gregorian values are equal as `eq` says
        // (F&O §15.3.1), with the implicit timezone — not as the order of the
        // schema facets, which their own `PartialEq` is (`P400Y` equals
        // `P146097D` there, a value without a timezone equals none with one).
        if is_temporal_type(left.type_code) || is_gregorian_code(left.type_code) {
            return false;
        }
        left == right
    }

    fn item_equal(&self, left: &XmlValue, right: &XmlValue) -> bool {
        let left = self.normalized_item_value(left);
        let right = self.normalized_item_value(right);

        if self.values_equal_or_nan(&left, &right) {
            return true;
        }

        // `eq` under this comparer's collation: a string pair is compared with
        // it, every other pair ignores it.
        if let Ok(result) = eval_binary_collated(
            BinaryOpKind::ValueEq,
            &left,
            &right,
            self.collation,
            self.implicit_timezone,
        ) {
            return result.as_boolean().unwrap_or(false);
        }

        false
    }

    fn is_whitespace_node<N: DomNavigator>(&self, nav: &N) -> bool {
        if !self.ignore_whitespace || !nav.node_type().is_text_like() {
            return false;
        }
        nav.value().chars().all(is_xml_whitespace)
    }

    /// Deep equality of two nodes, their content included.
    fn node_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        match self.node_step(left, right) {
            Step::Done(equal) => equal,
            Step::Children(kinds) => self.children_equal(left, right, kinds),
        }
    }

    /// The part of [`node_equal`](Self::node_equal) that does not descend:
    /// the kinds, names, attributes, values and typed values of the two nodes.
    /// What it leaves undecided is the pair's children.
    fn node_step<N: DomNavigator>(&self, left: &N, right: &N) -> Step {
        if left.node_type() != right.node_type() {
            return Step::Done(false);
        }

        match left.node_type() {
            DomNodeType::Element => self.element_step(left, right),
            DomNodeType::Attribute => Step::Done(self.attribute_equal(left, right)),
            DomNodeType::Namespace => Step::Done(self.namespace_equal(left, right)),
            DomNodeType::Text
            | DomNodeType::Whitespace
            | DomNodeType::SignificantWhitespace
            | DomNodeType::Comment => Step::Done(self.text_equal(&left.value(), &right.value())),
            DomNodeType::ProcessingInstruction => {
                Step::Done(self.processing_instruction_equal(left, right))
            }
            // Document nodes: their content is their children.
            _ => Step::Children(ChildKinds::Significant),
        }
    }

    fn element_step<N: DomNavigator>(&self, left: &N, right: &N) -> Step {
        if left.local_name() != right.local_name() || left.namespace_uri() != right.namespace_uri()
        {
            return Step::Done(false);
        }

        let mut left_nav = left.clone();
        let mut right_nav = right.clone();
        if !self.element_attributes_equal(&mut left_nav, &mut right_nav) {
            return Step::Done(false);
        }

        if !self.function_rules {
            return Step::Children(ChildKinds::Significant);
        }

        self.element_content_step(left, right)
    }

    /// Clauses (2) and (4) of the element rule (F&O §15.3.1).
    ///
    /// Clause (2) demands that the two elements are *both* annotated as having
    /// simple content or *both* as having complex content. Clause (4) then
    /// demands one of four cases, each of which requires the *same* case on
    /// both sides — so an element-only element and a mixed one are not
    /// deep-equal even when their element children match, because no case of
    /// clause (4) covers the pair.
    ///
    /// An element whose case cannot be told ([`ElementContent::Unknown`], a
    /// nilled element of a complex type with no schema set) is compared as
    /// being in the other element's case. The comparison that case prescribes
    /// is then right for it: a nilled element has an empty typed value and no
    /// element or text children, which is all 4(a), 4(b) and 4(c) look at. Two
    /// such elements are compared as mixed, and are equal. What is lost is
    /// clause (2): without a schema set a nilled element-only element can
    /// compare equal to a nilled simple-content one — the price of not
    /// knowing.
    fn element_content_step<N: DomNavigator>(&self, left: &N, right: &N) -> Step {
        let pair = match (self.element_content(left), self.element_content(right)) {
            (ElementContent::Unknown, ElementContent::Unknown) => {
                (ElementContent::Mixed, ElementContent::Mixed)
            }
            (ElementContent::Unknown, known) | (known, ElementContent::Unknown) => (known, known),
            known => known,
        };
        match pair {
            // 4(a): compare the typed values.
            (ElementContent::Simple, ElementContent::Simple) => {
                Step::Done(self.typed_values_equal(left, right))
            }
            // 4(b) / 4(d): compare only the child elements.
            (ElementContent::ElementOnly, ElementContent::ElementOnly) => {
                Step::Children(ChildKinds::Elements)
            }
            // 4(c): compare `(*|text())`, which is what the significant
            // children are once comment and PI children are skipped.
            (ElementContent::Mixed, ElementContent::Mixed) => {
                Step::Children(ChildKinds::Significant)
            }
            // A simple/complex mismatch fails clause (2); an element-only
            // against a mixed element matches no case of clause (4).
            _ => Step::Done(false),
        }
    }

    /// The content case of an element, read from its type annotation.
    ///
    /// The type annotation alone separates the cases, given the schema:
    /// a simple type, or a complex type whose content is text-only, is simple
    /// content; the other complex content kinds are element-only, mixed or
    /// empty; no annotation at all is `xs:untyped`, which is mixed.
    ///
    /// Without a schema set the content kind of a complex type cannot be read,
    /// so the node's typed value decides simple-vs-complex on its own and
    /// complex content is treated as mixed — the same answer an unvalidated
    /// document gives. A nilled element is the exception: its typed value is
    /// the empty sequence whether its type has simple or complex content, so
    /// it is [`ElementContent::Unknown`].
    fn element_content<N: DomNavigator>(&self, nav: &N) -> ElementContent {
        match nav.type_annotation() {
            None => ElementContent::Mixed,
            Some(TypeKey::Simple(_)) => ElementContent::Simple,
            Some(TypeKey::Complex(key)) => match self.complex_content_type(key) {
                Some(ContentType::TextOnly) => ElementContent::Simple,
                Some(ContentType::ElementOnly) | Some(ContentType::Empty) => {
                    ElementContent::ElementOnly
                }
                Some(ContentType::Mixed) => ElementContent::Mixed,
                None => match nav.typed_value() {
                    TypedValue::Value(_) => ElementContent::Simple,
                    TypedValue::Nilled => ElementContent::Unknown,
                    TypedValue::Untyped | TypedValue::Absent => ElementContent::Mixed,
                },
            },
        }
    }

    /// The content kind of a complex type, or `None` when there is no schema
    /// set to ask.
    ///
    /// The key comes from the node's annotation and is looked up with `get`,
    /// not indexing, so a key from some *other* schema set cannot panic. Like
    /// the rest of schema-aware XPath — `schema-element()`, `element(*, T)` —
    /// this assumes the static context's schema set is the one the document
    /// was validated against.
    fn complex_content_type(&self, key: ComplexTypeKey) -> Option<ContentType> {
        let schema_set = self.schema_set?;
        let ct_data = schema_set.arenas.complex_types.get(key)?;
        Some(determine_content_type(schema_set, ct_data))
    }

    /// Clause 4(a): the two elements' typed values are deep-equal.
    ///
    /// A typed value is a sequence (XDM §3.3.1.2): a nilled element's is the
    /// empty sequence, and a list-typed element's has one item per member.
    /// The two sequences are compared by [`atoms_equal`](Self::atoms_equal).
    fn typed_values_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        match (atomize::node_atoms(left), atomize::node_atoms(right)) {
            (Ok(left_atoms), Ok(right_atoms)) => self.atoms_equal(left_atoms, right_atoms),
            _ => false,
        }
    }

    /// Two typed values are deep-equal when they have the same length and their
    /// items are pairwise deep-equal under the atomic rule — `eq`, with NaN
    /// equal to NaN (F&O §15.3.1). Whatever list type carries the members plays
    /// no part, so an `xs:IDREFS` value and a list-of-`xs:IDREF` value with the
    /// same members are deep-equal.
    fn atoms_equal(&self, mut left: Atoms, mut right: Atoms) -> bool {
        if left.len() != right.len() {
            return false;
        }
        loop {
            match (left.next(), right.next()) {
                (Some(left_value), Some(right_value)) => {
                    if !self.item_equal(&left_value, &right_value) {
                        return false;
                    }
                }
                (None, None) => return true,
                _ => return false,
            }
        }
    }

    /// Whether the children of `left` and `right`, paired up as `kinds` says,
    /// are deep-equal pair by pair, and both run out together.
    ///
    /// The walk is iterative: a pair that needs its own children compared
    /// pushes a frame onto an explicit stack instead of calling, so the depth
    /// of the two trees costs heap, not Rust stack. Pairs are compared in
    /// document order, depth first, and the first unequal pair ends the walk —
    /// the order and the answer of a recursive comparison.
    fn children_equal<N: DomNavigator>(&self, left: &N, right: &N, kinds: ChildKinds) -> bool {
        let mut stack = vec![ChildPairs::new(left, right, kinds)];
        while let Some(frame) = stack.last_mut() {
            let (left_child, right_child) = match frame.kinds {
                ChildKinds::Significant => (
                    self.next_significant_child(&mut frame.left),
                    self.next_significant_child(&mut frame.right),
                ),
                ChildKinds::Elements => (
                    next_child_element(&mut frame.left),
                    next_child_element(&mut frame.right),
                ),
            };
            match (left_child, right_child) {
                (None, None) => {
                    stack.pop();
                }
                (Some(_), None) | (None, Some(_)) => return false,
                (Some(left_child), Some(right_child)) => {
                    match self.node_step(&left_child, &right_child) {
                        Step::Done(true) => {}
                        Step::Done(false) => return false,
                        Step::Children(kinds) => {
                            stack.push(ChildPairs::new(&left_child, &right_child, kinds));
                        }
                    }
                }
            }
        }
        true
    }

    fn element_attributes_equal<N: DomNavigator>(&self, left: &mut N, right: &mut N) -> bool {
        if left.has_attributes() != right.has_attributes() {
            return false;
        }

        if !left.has_attributes() {
            return true;
        }

        let left_count = count_attributes(left);
        let right_count = count_attributes(right);
        if left_count != right_count {
            return false;
        }

        if left.move_to_first_attribute() {
            loop {
                let mut found = false;
                if right.move_to_first_attribute() {
                    loop {
                        if self.attribute_equal(left, right) {
                            found = true;
                            break;
                        }
                        if !right.move_to_next_attribute() {
                            break;
                        }
                    }
                    right.move_to_parent();
                }

                if !found {
                    left.move_to_parent();
                    return false;
                }

                if !left.move_to_next_attribute() {
                    break;
                }
            }
            left.move_to_parent();
        }

        true
    }

    /// Two processing-instruction nodes are deep-equal when their names and
    /// their string values are equal (F&O §15.3.1). The *name* is a name, so it
    /// is compared by codepoints whatever the collation; the content is a
    /// string value, so it is not.
    fn processing_instruction_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        left.local_name() == right.local_name()
            && self.collated_equal(&left.value(), &right.value())
    }

    /// Deep equality for two namespace nodes.
    ///
    /// A namespace node has no children, so the generic child-sequence
    /// comparison would make every pair of them equal. Its content is its
    /// *name* — the prefix, an `xs:NCName`, absent (reported as the empty
    /// string by every [`DomNavigator`] backend) for a default-namespace
    /// binding — and its *string value*, the bound namespace URI. Both must
    /// match.
    ///
    /// Neither part is document text, so `ignore_whitespace` deliberately does
    /// not apply: a namespace URI that differs by whitespace is a different
    /// URI.
    fn namespace_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        // The prefix is the node's *name*, so it is never collated. The bound
        // URI is the node's string value, and XPath 2.0 §B.1 is explicit that
        // "functions and operators that compare strings using the default
        // collation also compare xs:anyURI values using the default
        // collation", so it is.
        left.local_name() == right.local_name()
            && self.collated_equal(&left.value_ref(), &right.value_ref())
    }

    fn attribute_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        if left.local_name() != right.local_name() || left.namespace_uri() != right.namespace_uri()
        {
            return false;
        }

        // Attributes cannot be Nilled/Absent; fall back to untyped on error.
        let left_value = crate::xpath::atomize::atomize_node(left)
            .ok()
            .flatten()
            .unwrap_or_else(|| XmlValue::untyped(left.value()));
        let right_value = crate::xpath::atomize::atomize_node(right)
            .ok()
            .flatten()
            .unwrap_or_else(|| XmlValue::untyped(right.value()));

        if self.function_rules {
            // F&O §15.3.1: two attributes are deep-equal when their names are
            // equal and their *typed values* are deep-equal — the atomic rule,
            // i.e. `eq` with NaN equal to NaN. `item_equal` is that rule; it is
            // what the free-standing atomic arm of `deep_equal_iter` applies,
            // so the same two values now get the same answer whether they
            // arrived as an attribute's typed value or as a sequence item.
            // An untyped attribute is `xs:untypedAtomic` and compares as a
            // string either way, so nothing moves for unvalidated documents.
            // A list-typed attribute's typed value is the sequence of its
            // members, compared item by item.
            self.atoms_equal(Atoms::of(left_value), Atoms::of(right_value))
        } else {
            self.values_equal_or_nan(&left_value, &right_value)
        }
    }

    /// Deep equality of the **children** of two navigator positions.
    ///
    /// This compares the two child sequences pairwise; it deliberately does
    /// *not* look at the two positions themselves, so their names, attributes
    /// and node kinds play no part. On two document nodes that is exactly
    /// `fn:deep-equal`, whose content is its children; on two elements it is
    /// a *content* comparison.
    fn deep_equal<N: DomNavigator>(&self, left: &N, right: &N) -> bool {
        self.children_equal(left, right, ChildKinds::Significant)
    }

    /// Deep equality for two XPath item iterators.
    pub(crate) fn deep_equal_iter<I>(&self, left: &I, right: &I) -> Result<bool, XPathError>
    where
        I: XmlNodeIterator,
    {
        let mut left_iter = left.clone();
        let mut right_iter = right.clone();

        loop {
            let left_has = left_iter.move_next()?;
            let right_has = right_iter.move_next()?;
            if left_has != right_has {
                return Ok(false);
            }
            if !left_has {
                return Ok(true);
            }

            let left_item = left_iter.current();
            let right_item = right_iter.current();

            match (left_item, right_item) {
                (Some(XmlItemRef::Node(left_node)), Some(XmlItemRef::Node(right_node))) => {
                    if !self.node_equal(left_node, right_node) {
                        return Ok(false);
                    }
                }
                (Some(XmlItemRef::Atomic(left_value)), Some(XmlItemRef::Atomic(right_value))) => {
                    if !self.item_equal(left_value, right_value) {
                        return Ok(false);
                    }
                }
                _ => return Ok(false),
            }
        }
    }

    fn next_significant_child<N: DomNavigator>(&self, iter: &mut ChildIter<N>) -> Option<N> {
        while let Some(nav) = iter.next() {
            if self.is_whitespace_node(&nav) {
                continue;
            }
            if self.function_rules && is_ignorable_child(&nav) {
                continue;
            }
            return Some(nav);
        }
        None
    }
}

/// What [`NodeComparer::node_step`] leaves to do for a pair of nodes.
enum Step {
    /// The pair is decided.
    Done(bool),
    /// The pair is deep-equal when its children are, paired up as the
    /// [`ChildKinds`] say.
    Children(ChildKinds),
}

/// Which children of two nodes a content comparison pairs up.
#[derive(Debug, Clone, Copy)]
enum ChildKinds {
    /// Every child but the whitespace-only text the comparer ignores and,
    /// under the `fn:deep-equal` rules, comments and PIs: a document node's
    /// content, a mixed element's `(*|text())` (clause 4(c)), and the whole
    /// content of an element under the published [`TreeComparer`] rules.
    Significant,
    /// Child elements only: element-only and empty content (clauses 4(b) and
    /// 4(d)). Text children — which in element-only content can only be
    /// whitespace — are not looked at, and neither are comments and PIs.
    Elements,
}

/// One level of [`NodeComparer::children_equal`]'s walk: the children of two
/// nodes still to compare.
struct ChildPairs<N: DomNavigator> {
    left: ChildIter<N>,
    right: ChildIter<N>,
    kinds: ChildKinds,
}

impl<N: DomNavigator> ChildPairs<N> {
    fn new(left: &N, right: &N, kinds: ChildKinds) -> Self {
        Self {
            left: ChildIter::new(left.clone()),
            right: ChildIter::new(right.clone()),
            kinds,
        }
    }
}

/// Whether a child node plays no part in `fn:deep-equal`.
///
/// F&O §15.3.1 compares a document or element node's `$i/(*|text())`, a
/// sequence that holds neither comment nor processing-instruction children. As
/// the spec's own note puts it, the content of a comment or PI matters only
/// when it is itself an item of the two sequences being compared; as a
/// *descendant* of a compared item it does not affect the result.
///
/// What such a child still does is **split text**: `<a>x<!--c-->y</a>` has two
/// text children, `x` and `y`, and is therefore not deep-equal to `<a>xy</a>`,
/// which has one. Skipping the comment here leaves both text nodes in place
/// and does not merge them, so that distinction survives.
fn is_ignorable_child<N: DomNavigator>(nav: &N) -> bool {
    matches!(
        nav.node_type(),
        DomNodeType::Comment | DomNodeType::ProcessingInstruction
    )
}

/// The next child **element**, skipping every other child kind.
fn next_child_element<N: DomNavigator>(iter: &mut ChildIter<N>) -> Option<N> {
    while let Some(nav) = iter.next() {
        if nav.node_type() == DomNodeType::Element {
            return Some(nav);
        }
    }
    None
}

fn count_attributes<N: DomNavigator>(nav: &mut N) -> usize {
    let mut count = 0;
    if nav.move_to_first_attribute() {
        loop {
            count += 1;
            if !nav.move_to_next_attribute() {
                break;
            }
        }
        nav.move_to_parent();
    }
    count
}

#[derive(Clone)]
struct ChildIter<N: DomNavigator> {
    nav: N,
    started: bool,
    done: bool,
}

impl<N: DomNavigator> ChildIter<N> {
    fn new(nav: N) -> Self {
        Self {
            nav,
            started: false,
            done: false,
        }
    }

    fn next(&mut self) -> Option<N> {
        if self.done {
            return None;
        }

        if !self.started {
            self.started = true;
            if !self.nav.move_to_first_child() {
                self.done = true;
                return None;
            }
            return Some(self.nav.clone());
        }

        if self.nav.move_to_next_sibling() {
            Some(self.nav.clone())
        } else {
            self.done = true;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use num_bigint::BigInt;
    use rust_decimal::Decimal;

    use crate::navigator::{NamespaceAxisScope, RoXmlNavigator};
    use crate::xpath::iterator::{VecNodeIterator, XmlItem};

    /// The document element of `doc`.
    fn ro_element<'d>(doc: &'d roxmltree::Document<'d>) -> RoXmlNavigator<'d> {
        let mut nav = RoXmlNavigator::new(doc);
        assert!(nav.move_to_first_child(), "a document element");
        nav
    }

    /// The namespace node with prefix `prefix` (`""` = the default binding) on
    /// the document element of `doc`.
    fn ro_namespace<'d>(doc: &'d roxmltree::Document<'d>, prefix: &str) -> RoXmlNavigator<'d> {
        let mut nav = ro_element(doc);
        assert!(
            nav.move_to_first_namespace(NamespaceAxisScope::Local),
            "a locally declared namespace",
        );
        loop {
            if nav.local_name() == prefix {
                return nav;
            }
            assert!(
                nav.move_to_next_namespace(NamespaceAxisScope::Local),
                "a namespace node with prefix {prefix:?}",
            );
        }
    }

    /// `fn:deep-equal` over two one-item node sequences, through the same
    /// entry point the function itself uses.
    fn nodes_deep_equal<N: DomNavigator>(left: N, right: N) -> bool {
        sequences_deep_equal(
            &NodeComparer::deep_equal_function(None, CollationRef::Codepoint, TimezoneOffset::UTC),
            vec![XmlItem::Node(left)],
            vec![XmlItem::Node(right)],
        )
    }

    /// `fn:deep-equal` over two sequences, with an explicit comparer so that a
    /// schema-aware one can be used.
    fn sequences_deep_equal<N: DomNavigator>(
        comparer: &NodeComparer<'_>,
        left: Vec<XmlItem<N>>,
        right: Vec<XmlItem<N>>,
    ) -> bool {
        let left: VecNodeIterator<N> = VecNodeIterator::new(left);
        let right: VecNodeIterator<N> = VecNodeIterator::new(right);
        comparer
            .deep_equal_iter(&left, &right)
            .expect("comparing two node sequences does not raise")
    }

    /// The document node of `doc`.
    fn ro_root<'d>(doc: &'d roxmltree::Document<'d>) -> RoXmlNavigator<'d> {
        RoXmlNavigator::new(doc)
    }

    /// The `index`-th child (0-based, every kind counted) of the document
    /// element of `doc`.
    fn ro_child<'d>(doc: &'d roxmltree::Document<'d>, index: usize) -> RoXmlNavigator<'d> {
        let mut nav = ro_element(doc);
        assert!(nav.move_to_first_child(), "a child node");
        for _ in 0..index {
            assert!(nav.move_to_next_sibling(), "a child node at index {index}");
        }
        nav
    }

    #[test]
    fn test_deep_equal_ignores_whitespace_nodes() {
        let comparer = TreeComparer::with_ignore_whitespace(true);
        let doc1 = roxmltree::Document::parse("<root>\n  <a>1</a>\n  <b>2</b>\n</root>")
            .expect("parse xml");
        let doc2 = roxmltree::Document::parse("<root><a>1</a><b>2</b></root>").expect("parse xml");
        let nav1 = RoXmlNavigator::new(&doc1);
        let nav2 = RoXmlNavigator::new(&doc2);

        assert!(comparer.deep_equal(&nav1, &nav2));
    }

    #[test]
    fn test_deep_equal_detects_whitespace_when_enabled() {
        let comparer = TreeComparer::new();
        let doc1 = roxmltree::Document::parse("<root>\n  <a>1</a>\n</root>").expect("parse xml");
        let doc2 = roxmltree::Document::parse("<root><a>1</a></root>").expect("parse xml");
        let nav1 = RoXmlNavigator::new(&doc1);
        let nav2 = RoXmlNavigator::new(&doc2);

        assert!(!comparer.deep_equal(&nav1, &nav2));
    }

    #[test]
    fn test_deep_equal_attributes_order_insensitive() {
        let comparer = TreeComparer::new();
        let doc1 = roxmltree::Document::parse("<root b=\"2\" a=\"1\"/>").expect("parse xml");
        let doc2 = roxmltree::Document::parse("<root a=\"1\" b=\"2\"/>").expect("parse xml");
        let nav1 = RoXmlNavigator::new(&doc1);
        let nav2 = RoXmlNavigator::new(&doc2);

        assert!(comparer.deep_equal(&nav1, &nav2));
    }

    #[test]
    fn test_deep_equal_iter_uses_value_eq() {
        let comparer = TreeComparer::new();
        let left: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(1)))]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::decimal(Decimal::new(1, 0)))]);

        assert!(comparer.deep_equal_iter(&left, &right).unwrap());
    }

    #[test]
    fn test_deep_equal_iter_nan() {
        let comparer = TreeComparer::new();
        let left: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::double(f64::NAN))]);
        let right: VecNodeIterator<RoXmlNavigator<'static>> =
            VecNodeIterator::new(vec![XmlItem::Atomic(XmlValue::float(f32::NAN))]);

        assert!(comparer.deep_equal_iter(&left, &right).unwrap());
    }

    /// Pins the contract of the public [`TreeComparer::deep_equal`]: it
    /// compares the *children* of the two positions, not the positions
    /// themselves. Two callers in this crate depend on that — a document-root
    /// comparison, where it coincides with `fn:deep-equal`, and a
    /// copied-subtree content check. This test characterises existing
    /// behaviour; it is not a defect test.
    #[test]
    fn deep_equal_compares_children_not_the_two_positions() {
        let left = roxmltree::Document::parse(r#"<a x="1">t</a>"#).expect("parse xml");
        let right = roxmltree::Document::parse(r#"<b y="2">t</b>"#).expect("parse xml");

        // Different name, different attributes — same children.
        assert!(TreeComparer::new().deep_equal(&ro_element(&left), &ro_element(&right)));
        // Compared as items, the same pair is not deep-equal.
        assert!(!nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    // ── Namespace nodes ───────────────────────────────────────────────
    //
    // Two namespace nodes are deep-equal exactly when their names — the
    // prefix, absent for a default-namespace binding — are equal and their
    // string values, the bound namespace URI, are equal.

    #[test]
    fn namespace_nodes_with_the_same_prefix_and_uri_are_deep_equal() {
        let left = roxmltree::Document::parse(r#"<r xmlns:a="http://x/"/>"#).expect("parse xml");
        let right = roxmltree::Document::parse(r#"<r xmlns:a="http://x/"/>"#).expect("parse xml");

        assert!(nodes_deep_equal(
            ro_namespace(&left, "a"),
            ro_namespace(&right, "a"),
        ));
    }

    #[test]
    fn namespace_nodes_differing_only_in_prefix_are_not_deep_equal() {
        let doc = roxmltree::Document::parse(r#"<r xmlns:a="http://x/" xmlns:b="http://x/"/>"#)
            .expect("parse xml");

        assert!(!nodes_deep_equal(
            ro_namespace(&doc, "a"),
            ro_namespace(&doc, "b"),
        ));
    }

    #[test]
    fn namespace_nodes_differing_only_in_uri_are_not_deep_equal() {
        let left = roxmltree::Document::parse(r#"<r xmlns:a="http://x/"/>"#).expect("parse xml");
        let right = roxmltree::Document::parse(r#"<r xmlns:a="http://y/"/>"#).expect("parse xml");

        assert!(!nodes_deep_equal(
            ro_namespace(&left, "a"),
            ro_namespace(&right, "a"),
        ));
    }

    #[test]
    fn a_default_namespace_node_is_not_deep_equal_to_a_prefixed_one() {
        let doc = roxmltree::Document::parse(r#"<r xmlns="http://x/" xmlns:a="http://x/"/>"#)
            .expect("parse xml");

        assert!(!nodes_deep_equal(
            ro_namespace(&doc, ""),
            ro_namespace(&doc, "a"),
        ));
    }

    #[test]
    fn default_namespace_nodes_with_the_same_uri_are_deep_equal() {
        let left = roxmltree::Document::parse(r#"<r xmlns="http://x/"/>"#).expect("parse xml");
        let right = roxmltree::Document::parse(r#"<r xmlns="http://x/"/>"#).expect("parse xml");

        assert!(nodes_deep_equal(
            ro_namespace(&left, ""),
            ro_namespace(&right, ""),
        ));
    }

    #[test]
    fn default_namespace_nodes_with_different_uris_are_not_deep_equal() {
        let left = roxmltree::Document::parse(r#"<r xmlns="http://x/"/>"#).expect("parse xml");
        let right = roxmltree::Document::parse(r#"<r xmlns="http://y/"/>"#).expect("parse xml");

        assert!(!nodes_deep_equal(
            ro_namespace(&left, ""),
            ro_namespace(&right, ""),
        ));
    }

    #[test]
    fn a_namespace_node_is_not_deep_equal_to_another_kind_with_the_same_string_value() {
        let doc =
            roxmltree::Document::parse(r#"<r xmlns:a="http://x/" a="http://x/">http://x/</r>"#)
                .expect("parse xml");
        let namespace = ro_namespace(&doc, "a");

        let mut attribute = RoXmlNavigator::new(&doc);
        assert!(attribute.move_to_first_child(), "a document element");
        assert!(attribute.move_to_first_attribute(), "an attribute");

        let mut text = RoXmlNavigator::new(&doc);
        assert!(text.move_to_first_child(), "a document element");
        assert!(text.move_to_first_child(), "a text node");

        assert_eq!(namespace.value(), attribute.value());
        assert_eq!(namespace.value(), text.value());
        assert!(!nodes_deep_equal(namespace.clone(), attribute));
        assert!(!nodes_deep_equal(namespace, text));
    }

    #[test]
    fn a_sequence_of_namespace_nodes_is_compared_item_by_item() {
        let doc = roxmltree::Document::parse(r#"<r xmlns:a="http://x/" xmlns:b="http://y/"/>"#)
            .expect("parse xml");
        let comparer = TreeComparer::new();

        let pair = |first: &str, second: &str| -> VecNodeIterator<RoXmlNavigator<'_>> {
            VecNodeIterator::new(vec![
                XmlItem::Node(ro_namespace(&doc, first)),
                XmlItem::Node(ro_namespace(&doc, second)),
            ])
        };

        assert!(comparer
            .deep_equal_iter(&pair("a", "b"), &pair("a", "b"))
            .unwrap());
        assert!(!comparer
            .deep_equal_iter(&pair("a", "b"), &pair("b", "a"))
            .unwrap());
    }

    #[test]
    fn namespace_nodes_of_a_buffer_document_follow_the_same_rule() {
        use crate::document::BufferDocument;
        use crate::namespace::NameTable;
        use bumpalo::Bump;

        let arena = Bump::new();
        let names = NameTable::new();
        let parse = |xml: &'static str| {
            BufferDocument::from_reader_default(xml.as_bytes(), &arena, &names)
                .expect("the fixture parses")
        };

        // The navigator borrows the document, so keep every document alive.
        let docs = [
            parse(r#"<r xmlns:a="http://x/"/>"#),
            parse(r#"<r xmlns:a="http://x/"/>"#),
            parse(r#"<r xmlns:b="http://x/"/>"#),
            parse(r#"<r xmlns:a="http://y/"/>"#),
            parse(r#"<r xmlns="http://x/"/>"#),
            parse(r#"<r xmlns="http://x/"/>"#),
        ];

        let namespace = |index: usize| {
            let mut nav = docs[index].create_navigator();
            assert!(nav.move_to_first_child(), "a document element");
            assert!(
                nav.move_to_first_namespace(NamespaceAxisScope::Local),
                "a locally declared namespace",
            );
            nav
        };

        assert!(
            nodes_deep_equal(namespace(0), namespace(1)),
            "same prefix, same URI"
        );
        assert!(
            !nodes_deep_equal(namespace(0), namespace(2)),
            "different prefix"
        );
        assert!(
            !nodes_deep_equal(namespace(0), namespace(3)),
            "different URI"
        );
        assert!(
            !nodes_deep_equal(namespace(0), namespace(4)),
            "prefixed vs default"
        );
        assert!(
            nodes_deep_equal(namespace(4), namespace(5)),
            "default binding, same URI"
        );
    }

    // ── Comments and PIs among children (F&O §15.3.1) ─────────────────
    //
    // A document or element node's content is `$i/(*|text())`, which holds
    // neither comment nor processing-instruction children, so those children
    // play no part in `fn:deep-equal`. They remain significant when they are
    // themselves items of the two compared sequences.

    #[test]
    fn a_comment_child_of_an_element_is_ignored() {
        let left = roxmltree::Document::parse("<a>x<!--c--></a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a>x</a>").expect("parse xml");

        assert!(nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    #[test]
    fn an_only_child_comment_leaves_an_element_empty() {
        let left = roxmltree::Document::parse("<a><!--c--></a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a/>").expect("parse xml");

        assert!(nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    #[test]
    fn a_processing_instruction_child_of_an_element_is_ignored() {
        let left = roxmltree::Document::parse("<a>x<?p d?></a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a>x</a>").expect("parse xml");

        assert!(nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    #[test]
    fn comments_at_different_places_among_children_are_both_ignored() {
        let left = roxmltree::Document::parse("<a><!--p-->x</a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a>x<!--q--></a>").expect("parse xml");

        assert!(nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    #[test]
    fn comments_and_pis_among_document_children_are_ignored() {
        let left = roxmltree::Document::parse("<!--c--><?p d?><a/><!--e-->").expect("parse xml");
        let right = roxmltree::Document::parse("<a/>").expect("parse xml");

        assert!(nodes_deep_equal(ro_root(&left), ro_root(&right)));
    }

    #[test]
    fn comments_deeper_in_the_tree_are_ignored_too() {
        let left = roxmltree::Document::parse("<a><b>t<!--c--></b></a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a><b>t</b></a>").expect("parse xml");

        assert!(nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    /// The spec's own note: a comment among the children does not affect the
    /// result, but it still *splits* the text around it, and text nodes are
    /// not merged. `<a>x<!--c-->y</a>` has two text children where `<a>xy</a>`
    /// has one, so the two are not deep-equal.
    #[test]
    fn a_comment_between_two_texts_does_not_merge_them() {
        let left = roxmltree::Document::parse("<a>x<!--c-->y</a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a>xy</a>").expect("parse xml");

        assert!(!nodes_deep_equal(ro_element(&left), ro_element(&right)));
    }

    #[test]
    fn a_comment_item_is_compared_by_string_value() {
        let doc = roxmltree::Document::parse("<a><!--c--><!--c--><!--d--></a>").expect("parse xml");

        assert!(nodes_deep_equal(ro_child(&doc, 0), ro_child(&doc, 1)));
        assert!(!nodes_deep_equal(ro_child(&doc, 0), ro_child(&doc, 2)));
    }

    #[test]
    fn a_processing_instruction_item_is_compared_by_target_and_value() {
        let doc =
            roxmltree::Document::parse("<a><?p d?><?p d?><?q d?><?p e?></a>").expect("parse xml");

        assert!(nodes_deep_equal(ro_child(&doc, 0), ro_child(&doc, 1)));
        assert!(
            !nodes_deep_equal(ro_child(&doc, 0), ro_child(&doc, 2)),
            "different target"
        );
        assert!(
            !nodes_deep_equal(ro_child(&doc, 0), ro_child(&doc, 3)),
            "different value"
        );
    }

    /// The published [`TreeComparer`] keeps the stricter comparison: it is
    /// what the XQTS judge and the copy round-trip check use, and both need a
    /// comment or PI child to count. `with_ignore_whitespace(true)` is the
    /// judge's own setting.
    #[test]
    fn the_public_tree_comparer_still_compares_comments_and_pis() {
        let left = roxmltree::Document::parse("<a><!--x--></a>").expect("parse xml");
        let right = roxmltree::Document::parse("<a/>").expect("parse xml");
        let pi = roxmltree::Document::parse("<a><?p d?></a>").expect("parse xml");

        for comparer in [
            TreeComparer::new(),
            TreeComparer::with_ignore_whitespace(true),
        ] {
            assert!(
                !comparer.deep_equal(&ro_element(&left), &ro_element(&right)),
                "a comment child must still count"
            );
            assert!(
                !comparer.deep_equal(&ro_element(&pi), &ro_element(&right)),
                "a PI child must still count"
            );
            // The XQTS judge compares two *document roots* with exactly this
            // comparer (`tests/xqts/compare.rs`), so check that shape too.
            assert!(
                !comparer.deep_equal(&ro_root(&left), &ro_root(&right)),
                "the judge must still see the comment"
            );
        }

        // …and through the item entry point as well.
        let left_iter = VecNodeIterator::new(vec![XmlItem::Node(ro_element(&left))]);
        let right_iter = VecNodeIterator::new(vec![XmlItem::Node(ro_element(&right))]);
        assert!(!TreeComparer::new()
            .deep_equal_iter(&left_iter, &right_iter)
            .expect("no error"));
    }

    // ── Typed comparison (F&O §15.3.1 clauses 2, 3 and 4) ─────────────

    mod typed {
        use super::*;

        use bumpalo::Bump;

        use crate::document::{
            build_typed_document, BufferDocNavigator, BufferDocument, BufferDocumentOptions,
        };
        use crate::pipeline::load_and_process_schema;
        use crate::schema::SchemaSet;

        fn load_schema(xsd: &str) -> SchemaSet {
            let mut schema_set = SchemaSet::xsd11();
            load_and_process_schema(xsd.as_bytes(), "test.xsd", &mut schema_set, None)
                .expect("the fixture schema loads");
            schema_set
        }

        /// A schema-validated document, whose element and attribute nodes
        /// carry type annotations.
        fn typed_doc<'a>(
            xml: &str,
            arena: &'a Bump,
            schema_set: &'a SchemaSet,
        ) -> BufferDocument<'a> {
            build_typed_document(
                xml.as_bytes(),
                arena,
                schema_set,
                BufferDocumentOptions::default(),
            )
            .expect("the fixture document is built")
        }

        /// The document element.
        fn element<'a>(doc: &'a BufferDocument<'a>) -> BufferDocNavigator<'a> {
            let mut nav = doc.create_navigator();
            assert!(nav.move_to_first_child(), "a document element");
            nav
        }

        /// The first attribute of the document element.
        fn attribute<'a>(doc: &'a BufferDocument<'a>) -> BufferDocNavigator<'a> {
            let mut nav = element(doc);
            assert!(nav.move_to_first_attribute(), "an attribute");
            nav
        }

        fn deep_equal(
            schema_set: Option<&SchemaSet>,
            left: BufferDocNavigator<'_>,
            right: BufferDocNavigator<'_>,
        ) -> bool {
            sequences_deep_equal(
                &NodeComparer::deep_equal_function(
                    schema_set,
                    CollationRef::Codepoint,
                    TimezoneOffset::UTC,
                ),
                vec![XmlItem::Node(left)],
                vec![XmlItem::Node(right)],
            )
        }

        const INTEGER_ATTRIBUTE: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:element name="e">
                    <xs:complexType>
                        <xs:attribute name="n" type="xs:integer"/>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#;

        /// Clause (3): an attribute's *typed values* are compared, so the two
        /// lexical forms `1` and `01` of the same `xs:integer` are equal.
        #[test]
        fn a_typed_attribute_is_compared_by_its_typed_value() {
            let schema_set = load_schema(INTEGER_ATTRIBUTE);
            let arena = Bump::new();
            let left = typed_doc(r#"<e n="1"/>"#, &arena, &schema_set);
            let right = typed_doc(r#"<e n="01"/>"#, &arena, &schema_set);

            assert!(deep_equal(
                Some(&schema_set),
                attribute(&left),
                attribute(&right),
            ));
        }

        /// The same two attributes without a schema are `xs:untypedAtomic`,
        /// which compares as a string — `1` and `01` are different strings.
        #[test]
        fn an_untyped_attribute_is_compared_as_a_string() {
            let arena = Bump::new();
            let names = crate::namespace::NameTable::new();
            let left =
                BufferDocument::from_reader_default(r#"<e n="1"/>"#.as_bytes(), &arena, &names)
                    .expect("the fixture parses");
            let right =
                BufferDocument::from_reader_default(r#"<e n="01"/>"#.as_bytes(), &arena, &names)
                    .expect("the fixture parses");

            assert!(!deep_equal(None, attribute(&left), attribute(&right)));
        }

        /// The A-7 audit's §5.5: an attribute's typed values went through
        /// plain value equality while a free-standing atomic item went through
        /// `eq`, so the same pair got two answers. `fn:deep-equal` now applies
        /// `eq` to both; the published `TreeComparer` keeps plain equality.
        #[test]
        fn typed_attributes_of_different_types_compare_with_eq() {
            let schema_set = load_schema(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="e">
                        <xs:complexType>
                            <xs:attribute name="n" type="xs:integer"/>
                        </xs:complexType>
                    </xs:element>
                </xs:schema>"#,
            );
            let decimal_set = load_schema(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="e">
                        <xs:complexType>
                            <xs:attribute name="n" type="xs:decimal"/>
                        </xs:complexType>
                    </xs:element>
                </xs:schema>"#,
            );
            let arena = Bump::new();
            let integer = typed_doc(r#"<e n="1"/>"#, &arena, &schema_set);
            let decimal = typed_doc(r#"<e n="1.0"/>"#, &arena, &decimal_set);

            assert!(
                deep_equal(Some(&schema_set), attribute(&integer), attribute(&decimal)),
                "xs:integer 1 eq xs:decimal 1.0"
            );
            assert!(
                !TreeComparer::new()
                    .deep_equal_iter(
                        &VecNodeIterator::new(vec![XmlItem::Node(attribute(&integer))]),
                        &VecNodeIterator::new(vec![XmlItem::Node(attribute(&decimal))]),
                    )
                    .expect("no error"),
                "the published comparer keeps plain value equality",
            );
        }

        /// Clause 4(a): two elements with simple content are compared by
        /// their typed values.
        #[test]
        fn simple_content_elements_are_compared_by_typed_value() {
            let schema_set = load_schema(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="e" type="xs:integer"/>
                </xs:schema>"#,
            );
            let arena = Bump::new();
            let left = typed_doc("<e>1</e>", &arena, &schema_set);
            let right = typed_doc("<e>01</e>", &arena, &schema_set);

            assert!(deep_equal(
                Some(&schema_set),
                element(&left),
                element(&right),
            ));
        }

        /// …and the same two elements with no schema are untyped, hence mixed
        /// complex content, hence compared by their text children.
        #[test]
        fn untyped_elements_are_compared_by_their_text() {
            let arena = Bump::new();
            let names = crate::namespace::NameTable::new();
            let left = BufferDocument::from_reader_default("<e>1</e>".as_bytes(), &arena, &names)
                .expect("the fixture parses");
            let right = BufferDocument::from_reader_default("<e>01</e>".as_bytes(), &arena, &names)
                .expect("the fixture parses");

            assert!(!deep_equal(None, element(&left), element(&right)));
        }

        /// Clause 4(b): with element-only content only the child *elements*
        /// are compared, so the whitespace between them does not matter —
        /// without `ignore_whitespace`, which `fn:deep-equal` never sets.
        #[test]
        fn element_only_content_ignores_text_children() {
            let schema_set = load_schema(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="e">
                        <xs:complexType>
                            <xs:sequence>
                                <xs:element name="c" type="xs:string"/>
                            </xs:sequence>
                        </xs:complexType>
                    </xs:element>
                </xs:schema>"#,
            );
            let arena = Bump::new();
            let left = typed_doc("<e>\n  <c>x</c>\n</e>", &arena, &schema_set);
            let right = typed_doc("<e><c>x</c></e>", &arena, &schema_set);
            let other = typed_doc("<e><c>y</c></e>", &arena, &schema_set);

            assert!(deep_equal(
                Some(&schema_set),
                element(&left),
                element(&right),
            ));
            assert!(
                !deep_equal(Some(&schema_set), element(&left), element(&other)),
                "the child elements themselves still count"
            );
        }

        /// Clause 4(c): with mixed content the `(*|text())` sequence is
        /// compared, so the whitespace *does* matter.
        #[test]
        fn mixed_content_compares_text_children() {
            let schema_set = load_schema(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="e">
                        <xs:complexType mixed="true">
                            <xs:sequence>
                                <xs:element name="c" type="xs:string"/>
                            </xs:sequence>
                        </xs:complexType>
                    </xs:element>
                </xs:schema>"#,
            );
            let arena = Bump::new();
            let left = typed_doc("<e>\n  <c>x</c>\n</e>", &arena, &schema_set);
            let right = typed_doc("<e><c>x</c></e>", &arena, &schema_set);

            assert!(!deep_equal(
                Some(&schema_set),
                element(&left),
                element(&right),
            ));
        }

        /// Clause (2): both elements must be annotated as having simple
        /// content or both as having complex content. An element with no
        /// annotation is `xs:untyped`, which is complex content.
        #[test]
        fn simple_content_is_never_deep_equal_to_complex_content() {
            let schema_set = load_schema(
                r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                    <xs:element name="e" type="xs:string"/>
                </xs:schema>"#,
            );
            let arena = Bump::new();
            let names = crate::namespace::NameTable::new();
            let typed = typed_doc("<e>x</e>", &arena, &schema_set);
            let untyped =
                BufferDocument::from_reader_default("<e>x</e>".as_bytes(), &arena, &names)
                    .expect("the fixture parses");

            assert!(!deep_equal(
                Some(&schema_set),
                element(&typed),
                element(&untyped),
            ));
        }

        /// One nillable `e` per parent, so that the elements compared share a
        /// name: under `s` it has a simple type, under `f` a complex type with
        /// simple content, under `p` a complex type with element-only content.
        const NILLABLE_E: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                <xs:complexType name="flag">
                    <xs:simpleContent>
                        <xs:extension base="xs:boolean"/>
                    </xs:simpleContent>
                </xs:complexType>
                <xs:complexType name="pair">
                    <xs:sequence>
                        <xs:element name="c" type="xs:string"/>
                    </xs:sequence>
                </xs:complexType>
                <xs:element name="doc">
                    <xs:complexType>
                        <xs:sequence>
                            <xs:element name="s">
                                <xs:complexType><xs:sequence>
                                    <xs:element name="e" type="xs:boolean" nillable="true"/>
                                </xs:sequence></xs:complexType>
                            </xs:element>
                            <xs:element name="f">
                                <xs:complexType><xs:sequence>
                                    <xs:element name="e" type="flag" nillable="true"/>
                                </xs:sequence></xs:complexType>
                            </xs:element>
                            <xs:element name="p">
                                <xs:complexType><xs:sequence>
                                    <xs:element name="e" type="pair" nillable="true"/>
                                </xs:sequence></xs:complexType>
                            </xs:element>
                        </xs:sequence>
                    </xs:complexType>
                </xs:element>
            </xs:schema>"#;

        const ALL_NILLED: &str = concat!(
            r#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#,
            r#"<s><e xsi:nil="true"/></s><f><e xsi:nil="true"/></f><p><e xsi:nil="true"/></p>"#,
            r#"</doc>"#
        );

        const NONE_NILLED: &str =
            "<doc><s><e>true</e></s><f><e>true</e></f><p><e><c>x</c></e></p></doc>";

        /// The `e` child of the document element's `parent` child.
        fn e_under<'a>(doc: &'a BufferDocument<'a>, parent: &str) -> BufferDocNavigator<'a> {
            let mut nav = element(doc);
            assert!(nav.move_to_child_name(parent, ""), "a {parent} child");
            assert!(nav.move_to_child_name("e", ""), "an e child of {parent}");
            nav
        }

        /// Clauses (2) and (4) alone, without the name and attribute checks
        /// that come first: a nilled element carries `xsi:nil="true"`, so its
        /// attributes already tell it from one that is not nilled.
        fn content_equal(
            schema_set: Option<&SchemaSet>,
            left: &BufferDocNavigator<'_>,
            right: &BufferDocNavigator<'_>,
        ) -> bool {
            let comparer = NodeComparer::deep_equal_function(
                schema_set,
                CollationRef::Codepoint,
                TimezoneOffset::UTC,
            );
            match comparer.element_content_step(left, right) {
                Step::Done(equal) => equal,
                Step::Children(kinds) => comparer.children_equal(left, right, kinds),
            }
        }

        /// Review finding: a nilled element whose complex type has simple
        /// content has the typed value of every nilled element, the empty
        /// sequence, so without a schema set it cannot be told from a nilled
        /// complex-content element. It is compared as having the other
        /// element's content kind — here simple, so the two empty typed values
        /// are compared, and are equal, with the schema set and without it.
        #[test]
        fn a_nilled_simple_content_element_equals_a_nilled_simple_typed_one() {
            let schema_set = load_schema(NILLABLE_E);
            let arena = Bump::new();
            let doc = typed_doc(ALL_NILLED, &arena, &schema_set);

            for schema in [None, Some(&schema_set)] {
                assert!(
                    deep_equal(schema, e_under(&doc, "s"), e_under(&doc, "f")),
                    "xs:boolean against an extension of it (schema set: {})",
                    schema.is_some(),
                );
                assert!(
                    deep_equal(schema, e_under(&doc, "f"), e_under(&doc, "s")),
                    "an extension of xs:boolean against xs:boolean (schema set: {})",
                    schema.is_some(),
                );
            }
        }

        /// Taking the other element's content kind is only lenient where
        /// nothing can differ: against a simple-typed element *with* a value,
        /// the nilled element's empty typed value is compared with that value,
        /// and the two are not equal.
        #[test]
        fn a_nilled_element_does_not_equal_a_simple_element_with_a_value() {
            let schema_set = load_schema(NILLABLE_E);
            let arena = Bump::new();
            let nilled = typed_doc(ALL_NILLED, &arena, &schema_set);
            let valued = typed_doc(NONE_NILLED, &arena, &schema_set);

            for schema in [None, Some(&schema_set)] {
                let flag = e_under(&nilled, "f");
                let boolean = e_under(&valued, "s");
                assert!(
                    !content_equal(schema, &flag, &boolean),
                    "schema set: {}",
                    schema.is_some()
                );
                assert!(
                    !content_equal(schema, &boolean, &flag),
                    "schema set: {}",
                    schema.is_some()
                );
                assert!(!deep_equal(schema, flag, boolean));
            }
        }

        /// With a schema set the content kind is read from the type, so a
        /// nilled element-only element is still never deep-equal to a nilled
        /// simple-content one (clause (2)). Without a schema set neither nilled
        /// complex-typed element can be told apart, and the leniency lets them
        /// compare equal — the price of not knowing.
        #[test]
        fn a_nilled_element_only_element_is_not_a_nilled_simple_one_with_a_schema_set() {
            let schema_set = load_schema(NILLABLE_E);
            let arena = Bump::new();
            let doc = typed_doc(ALL_NILLED, &arena, &schema_set);

            assert!(!deep_equal(
                Some(&schema_set),
                e_under(&doc, "p"),
                e_under(&doc, "s"),
            ));
            assert!(!deep_equal(
                Some(&schema_set),
                e_under(&doc, "s"),
                e_under(&doc, "p"),
            ));
            assert!(!deep_equal(
                Some(&schema_set),
                e_under(&doc, "p"),
                e_under(&doc, "f"),
            ));

            assert!(deep_equal(None, e_under(&doc, "p"), e_under(&doc, "s")));
            assert!(deep_equal(None, e_under(&doc, "s"), e_under(&doc, "p")));
            // Two elements whose kind cannot be told are compared as mixed:
            // neither has a child, so they are equal.
            assert!(deep_equal(None, e_under(&doc, "p"), e_under(&doc, "f")));
        }
    }
}
