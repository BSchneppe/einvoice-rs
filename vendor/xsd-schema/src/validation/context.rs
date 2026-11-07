//! Per-element validation state and validator state machine
//!
//! `ElementValidationState` holds all per-element context that is pushed/popped
//! as the validator enters/exits elements. `ValidatorState` enforces the correct
//! sequence of push-API calls.

use std::collections::HashSet;

#[cfg(feature = "xsd11")]
use std::collections::HashMap;

use std::hash::BuildHasherDefault;

use ahash::AHasher;

#[cfg(feature = "xsd11")]
use crate::ids::AttributeKey;
use crate::ids::{ElementKey, NameId, TypeKey};
use crate::types::value::XmlValue;

use super::content::ContentValidatorState;
use super::info::{ContentProcessing, ContentType, SchemaValidity, TypeSource};

/// Set of `(namespace, local-name)` attribute identities, hashed with `ahash`.
///
/// The keys are tuples of dense interned-name integers (`Option<NameId>`,
/// `NameId`), so `ahash` is used instead of SipHash to cheapen the per-element
/// `seen_attributes` inserts and required-attribute checks.
///
/// The build-hasher is the zero-sized [`BuildHasherDefault<AHasher>`] (ahash's
/// fixed-key default), **not** `RandomState`. A fresh `ElementValidationState`
/// is created for every element, and `RandomState`'s `Default` re-seeds a
/// hasher (an `ahash::RandomState::from_keys` fold) on *each* construction —
/// ~2.2% of pure-validation CPU, almost all of it on maps that stay empty (the
/// leaf elements carry no attributes). `BuildHasherDefault` is a ZST whose
/// `Default` is free, so empty per-element maps cost nothing to construct. The
/// keys are interned `NameId`s minted by our own name table (not attacker-
/// controlled hash bytes) in maps of at most a handful of entries, so the
/// HashDoS surface that motivates a randomized seed is negligible here.
pub type AttrNameSet = HashSet<(Option<NameId>, NameId), BuildHasherDefault<AHasher>>;

/// Map keyed by `(namespace, local-name)` attribute identity, hashed with the
/// same zero-sized fixed-key `ahash` build-hasher as [`AttrNameSet`].
#[cfg(feature = "xsd11")]
pub type AttrNameMap<V> = HashMap<(Option<NameId>, NameId), V, BuildHasherDefault<AHasher>>;

/// An inherited attribute value flowing from an ancestor element (XSD 1.1).
///
/// Stored in [`ElementValidationState::incoming_inherited`] and propagated
/// from parent to child on element open. See §3.3.5.6 *Inherited Attributes*.
#[cfg(feature = "xsd11")]
#[derive(Debug, Clone)]
pub struct InheritedAttributeValue {
    /// The attribute value (string form)
    pub value: String,
    /// The governing attribute declaration key, if known
    pub attribute_key: Option<AttributeKey>,
}

/// Per-element state pushed onto the validation stack
///
/// Each time `validate_element` is called, a new `ElementValidationState` is
/// created and pushed. It is popped on `validate_end_element`.
#[derive(Debug, Clone)]
pub struct ElementValidationState {
    /// Local name of the element
    pub local_name: NameId,
    /// Namespace URI of the element (None for no-namespace)
    pub namespace: Option<NameId>,
    /// Resolved element declaration, if found
    pub element_decl: Option<ElementKey>,
    /// Resolved schema type (simple or complex)
    pub schema_type: Option<TypeKey>,
    /// Content model state for this element's type
    pub content_state: ContentValidatorState,
    /// Content type classification (Empty, TextOnly, ElementOnly, Mixed)
    pub content_type: Option<ContentType>,
    /// Whether xsi:nil="true" was specified
    pub is_nil: bool,
    /// Whether the element value came from a default declaration
    pub is_default: bool,
    /// For union types: the actual member type that matched the value
    pub member_type: Option<TypeKey>,
    /// The parsed typed value from simple-type validation
    pub typed_value: Option<XmlValue>,
    /// The whitespace-normalized value (PSVI `[schema normalized value]`)
    pub normalized_value: Option<String>,
    /// Current validity status
    pub validity: SchemaValidity,
    /// Accumulated constraint codes for PSVI `[schema error code]`
    pub error_codes: Vec<&'static str>,
    /// True if any child element has `[validation attempted]` != Full
    pub any_child_not_full: bool,
    /// True if any child element has `[validation attempted]` != None
    pub any_child_not_none: bool,
    /// True if any attribute has `[validation attempted]` != Full
    pub any_attr_not_full: bool,
    /// True if any attribute has `[validation attempted]` != None
    pub any_attr_not_none: bool,
    /// Whether this element was strictly assessed (§5.2 key-sva)
    pub strictly_assessed: bool,
    /// Notation declaration resolved from a NOTATION-typed attribute (§3.14.5)
    pub notation: Option<crate::ids::NotationKey>,
    /// Namespace context snapshot for resolving NOTATION QNames during attribute validation
    pub ns_context: Option<crate::namespace::context::NamespaceContextSnapshot>,
    /// Unique serial number for this element (monotonically increasing).
    /// Used for XSD 1.1 ID/IDREF binding: same ID on the same owner element is
    /// allowed (§3.17.5.2).
    pub element_serial: u64,
    /// How to process wildcard-matched content
    pub process_contents: ContentProcessing,
    /// Effective base URI for this element (inherited from parent, possibly
    /// overridden by `xml:base`). Used to resolve relative schema-location
    /// hints in `xsi:schemaLocation` / `xsi:noNamespaceSchemaLocation`.
    pub base_uri: String,
    /// Whether `xml:base` has already been applied on this element.
    /// Prevents a duplicate `xml:base` attribute from overwriting the valid one.
    pub base_uri_set_by_xml_base: bool,
    /// Start index of this element's `xsi:schemaLocation` hints in the runtime buffer.
    pub schema_location_hint_start: usize,
    /// Start index of this element's `xsi:noNamespaceSchemaLocation` hints in the runtime buffer.
    pub no_namespace_schema_location_hint_start: usize,
    /// Set of (namespace, local_name) pairs for attributes already seen
    pub seen_attributes: AttrNameSet,
    /// Whether an ID-typed attribute has already been seen on this element.
    /// Used to enforce the "at most one ID-type attribute per element" rule
    /// (XSD 1.0 §3.4.4 / §3.5.6 ct-props-correct.5) at runtime, after
    /// wildcard-matched globals contribute.
    pub seen_id_attr: bool,
    /// Accumulated text content for the element
    pub text_content: String,
    /// Whether any text nodes have been seen
    pub has_text: bool,
    /// Whether any child element nodes have been seen
    pub has_element_children: bool,
    /// How the schema_type was determined
    pub type_source: Option<TypeSource>,
    /// Whether CTA selected a type (XSD 1.1)
    #[cfg(feature = "xsd11")]
    pub cta_selected: bool,
    /// Whether this element owns an assertion buffer frame (XSD 1.1)
    #[cfg(feature = "xsd11")]
    pub owns_assertion_buffer: bool,
    /// Whether this element has type alternatives (XSD 1.1)
    #[cfg(feature = "xsd11")]
    pub has_type_alternatives: bool,
    /// Collected attributes for type alternative XPath evaluation (XSD 1.1)
    #[cfg(feature = "xsd11")]
    pub collected_attributes: Vec<(Option<NameId>, NameId, String)>,
    /// Node ref of this element in the assertion fragment document (XSD 1.1).
    /// Saved during `detect_assertions_on_element` for CTA re-detection.
    #[cfg(feature = "xsd11")]
    pub assertion_element_ref: Option<u32>,
    /// **Incoming** inherited attributes: the PSVI `[inherited attributes]`
    /// for this element (XSD 1.1 §3.3.5.6, structures.html line 5200).
    ///
    /// Snapshot of potentially-inherited attribute values from ancestors,
    /// frozen at element open. This is what `get_inherited_attributes()`
    /// returns and what CTA XDM construction reads. Never mutated after
    /// `push_element()`.
    #[cfg(feature = "xsd11")]
    pub incoming_inherited: AttrNameMap<InheritedAttributeValue>,
    /// **Outgoing** inherited attributes: the propagation map for this
    /// element's descendants.
    ///
    /// Starts as a clone of `incoming_inherited`, then updated when this
    /// element has explicit or defaulted inheritable attributes (which
    /// shadow ancestor values per the nearest-owner rule,
    /// structures.html line 5205). Children clone this map as their
    /// `incoming_inherited`.
    #[cfg(feature = "xsd11")]
    pub outgoing_inherited: AttrNameMap<InheritedAttributeValue>,
}

impl ElementValidationState {
    /// Create a new element validation state with defaults
    pub fn new(local_name: NameId, namespace: Option<NameId>) -> Self {
        ElementValidationState {
            local_name,
            namespace,
            element_decl: None,
            schema_type: None,
            content_state: ContentValidatorState::Empty,
            content_type: None,
            is_nil: false,
            is_default: false,
            member_type: None,
            typed_value: None,
            normalized_value: None,
            validity: SchemaValidity::NotKnown,
            error_codes: Vec::new(),
            any_child_not_full: false,
            any_child_not_none: false,
            any_attr_not_full: false,
            any_attr_not_none: false,
            strictly_assessed: false,
            notation: None,
            ns_context: None,
            element_serial: 0,
            process_contents: ContentProcessing::Strict,
            base_uri: String::new(),
            base_uri_set_by_xml_base: false,
            schema_location_hint_start: 0,
            no_namespace_schema_location_hint_start: 0,
            seen_attributes: AttrNameSet::default(),
            seen_id_attr: false,
            text_content: String::new(),
            has_text: false,
            has_element_children: false,
            type_source: None,
            #[cfg(feature = "xsd11")]
            cta_selected: false,
            #[cfg(feature = "xsd11")]
            owns_assertion_buffer: false,
            #[cfg(feature = "xsd11")]
            has_type_alternatives: false,
            #[cfg(feature = "xsd11")]
            collected_attributes: Vec::new(),
            #[cfg(feature = "xsd11")]
            assertion_element_ref: None,
            #[cfg(feature = "xsd11")]
            incoming_inherited: AttrNameMap::default(),
            #[cfg(feature = "xsd11")]
            outgoing_inherited: AttrNameMap::default(),
        }
    }

    /// Reset this state for reuse by the element-state pool, leaving it exactly
    /// as a fresh [`new`](Self::new) for `local_name`/`namespace` — but keeping
    /// the heap capacity of the per-element collections so the next element
    /// avoids re-allocating them.
    ///
    /// Implemented by rebuilding through `Self::new` and then re-injecting only
    /// the (cleared) capacity-bearing collections. This is deliberately robust
    /// against future struct changes: any field added later is reset correctly
    /// by `Self::new` with no edit here, so pooled reuse can never silently
    /// carry stale state from a previous element.
    pub fn reset(&mut self, local_name: NameId, namespace: Option<NameId>) {
        // Salvage the capacity-bearing allocations before the wholesale reset,
        // clearing each so no value survives into the next element.
        let mut error_codes = std::mem::take(&mut self.error_codes);
        error_codes.clear();
        let mut base_uri = std::mem::take(&mut self.base_uri);
        base_uri.clear();
        let mut seen_attributes = std::mem::take(&mut self.seen_attributes);
        seen_attributes.clear();
        let mut text_content = std::mem::take(&mut self.text_content);
        text_content.clear();
        #[cfg(feature = "xsd11")]
        let collected_attributes = {
            let mut v = std::mem::take(&mut self.collected_attributes);
            v.clear();
            v
        };
        #[cfg(feature = "xsd11")]
        let incoming_inherited = {
            let mut m = std::mem::take(&mut self.incoming_inherited);
            m.clear();
            m
        };
        #[cfg(feature = "xsd11")]
        let outgoing_inherited = {
            let mut m = std::mem::take(&mut self.outgoing_inherited);
            m.clear();
            m
        };

        *self = Self::new(local_name, namespace);

        self.error_codes = error_codes;
        self.base_uri = base_uri;
        self.seen_attributes = seen_attributes;
        self.text_content = text_content;
        #[cfg(feature = "xsd11")]
        {
            self.collected_attributes = collected_attributes;
            self.incoming_inherited = incoming_inherited;
            self.outgoing_inherited = outgoing_inherited;
        }
    }
}

/// State machine for the validator's call sequence
///
/// Enforces that push-API methods are called in the correct order.
/// The valid transitions are:
///
/// ```text
/// None → Start → Element → Attribute* → EndOfAttributes → (Text|Whitespace)* → EndElement → ... → Finish
///                                                          ↑                      |
///                                                          └── (Element cycle) ────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorState {
    /// Initial state, no validation has started
    None,
    /// `validate_element` has been called for the root element
    Start,
    /// Inside an element (after `validate_element`)
    Element,
    /// Processing attributes (after `validate_attribute`)
    Attribute,
    /// After `validate_end_of_attributes`
    EndOfAttributes,
    /// After `validate_text`
    Text,
    /// After `validate_whitespace`
    Whitespace,
    /// After `validate_end_element`
    EndElement,
    /// After `end_validation` — no further calls allowed
    Finish,
}

impl ValidatorState {
    /// Check if `validate_element` can be called in this state
    pub fn can_start_element(&self) -> bool {
        matches!(
            self,
            ValidatorState::None
                | ValidatorState::Start
                | ValidatorState::EndOfAttributes
                | ValidatorState::Text
                | ValidatorState::Whitespace
                | ValidatorState::EndElement
        )
    }

    /// Check if `validate_attribute` can be called in this state
    pub fn can_validate_attribute(&self) -> bool {
        matches!(self, ValidatorState::Element | ValidatorState::Attribute)
    }

    /// Check if `validate_end_of_attributes` can be called in this state
    pub fn can_end_attributes(&self) -> bool {
        matches!(self, ValidatorState::Element | ValidatorState::Attribute)
    }

    /// Check if `validate_text` / `validate_whitespace` can be called in this state
    pub fn can_validate_text(&self) -> bool {
        matches!(
            self,
            ValidatorState::EndOfAttributes
                | ValidatorState::Text
                | ValidatorState::Whitespace
                | ValidatorState::EndElement
        )
    }

    /// Check if `validate_end_element` can be called in this state
    pub fn can_end_element(&self) -> bool {
        matches!(
            self,
            ValidatorState::EndOfAttributes
                | ValidatorState::Text
                | ValidatorState::Whitespace
                | ValidatorState::EndElement
        )
    }

    /// Check if `end_validation` can be called in this state
    pub fn can_finish(&self) -> bool {
        matches!(self, ValidatorState::EndElement | ValidatorState::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_validation_state_defaults() {
        let state = ElementValidationState::new(NameId(1), None);
        assert_eq!(state.local_name, NameId(1));
        assert!(state.namespace.is_none());
        assert!(state.element_decl.is_none());
        assert!(state.schema_type.is_none());
        assert!(state.content_type.is_none());
        assert!(!state.is_nil);
        assert!(!state.is_default);
        assert_eq!(state.validity, SchemaValidity::NotKnown);
        assert_eq!(state.process_contents, ContentProcessing::Strict);
        assert!(state.seen_attributes.is_empty());
        assert!(state.text_content.is_empty());
        assert!(!state.has_text);
        assert!(!state.has_element_children);
        assert!(state.type_source.is_none());
        #[cfg(feature = "xsd11")]
        assert!(!state.cta_selected);
    }

    #[test]
    fn test_element_validation_state_with_namespace() {
        let state = ElementValidationState::new(NameId(5), Some(NameId(10)));
        assert_eq!(state.local_name, NameId(5));
        assert_eq!(state.namespace, Some(NameId(10)));
    }

    /// Drift guard for the pooling `reset()`: after mutating a representative
    /// spread of fields (scalars, options, and every capacity-bearing
    /// collection), `reset()` must leave the state observably identical to a
    /// fresh `new()` — and it must not carry any stale collection entries.
    /// `reset()` rebuilds via `Self::new`, so this also protects future fields.
    #[test]
    fn test_reset_matches_new() {
        let mut state = ElementValidationState::new(NameId(1), None);
        // Scalars / options to non-default values.
        state.is_nil = true;
        state.is_default = true;
        state.validity = SchemaValidity::Invalid;
        state.process_contents = ContentProcessing::Skip;
        state.has_text = true;
        state.has_element_children = true;
        state.seen_id_attr = true;
        state.base_uri_set_by_xml_base = true;
        state.element_serial = 42;
        state.schema_location_hint_start = 7;
        state.no_namespace_schema_location_hint_start = 9;
        state.normalized_value = Some("normalized".to_string());
        state.content_state = ContentValidatorState::Simple;
        // Every capacity-bearing collection populated.
        state.error_codes.push("cvc-test");
        state.base_uri.push_str("http://example.com/base");
        state.text_content.push_str("some accumulated text");
        state.seen_attributes.insert((None, NameId(100)));
        #[cfg(feature = "xsd11")]
        {
            state.cta_selected = true;
            state.owns_assertion_buffer = true;
            state.has_type_alternatives = true;
            state.assertion_element_ref = Some(3);
            state
                .collected_attributes
                .push((None, NameId(1), "v".to_string()));
            state.incoming_inherited.insert(
                (None, NameId(2)),
                InheritedAttributeValue {
                    value: "a".to_string(),
                    attribute_key: None,
                },
            );
            state.outgoing_inherited.insert(
                (None, NameId(3)),
                InheritedAttributeValue {
                    value: "b".to_string(),
                    attribute_key: None,
                },
            );
        }

        state.reset(NameId(5), Some(NameId(10)));

        // Observably identical to a fresh construction for the new identity.
        let fresh = ElementValidationState::new(NameId(5), Some(NameId(10)));
        assert_eq!(format!("{state:?}"), format!("{fresh:?}"));

        // No stale entries survive in any reused collection.
        assert!(state.error_codes.is_empty());
        assert!(state.base_uri.is_empty());
        assert!(state.text_content.is_empty());
        assert!(state.seen_attributes.is_empty());
        #[cfg(feature = "xsd11")]
        {
            assert!(state.collected_attributes.is_empty());
            assert!(state.incoming_inherited.is_empty());
            assert!(state.outgoing_inherited.is_empty());
        }

        // Capacity is retained — the reason the pool exists.
        assert!(state.text_content.capacity() > 0);
        assert!(state.base_uri.capacity() > 0);
    }

    #[test]
    fn test_seen_attributes_dedup() {
        let mut state = ElementValidationState::new(NameId(1), None);
        let attr = (None, NameId(100));
        assert!(state.seen_attributes.insert(attr));
        // Second insert returns false — duplicate
        assert!(!state.seen_attributes.insert(attr));
        assert_eq!(state.seen_attributes.len(), 1);
    }

    #[test]
    fn test_validator_state_transitions() {
        // None -> can start element
        assert!(ValidatorState::None.can_start_element());
        assert!(!ValidatorState::None.can_validate_attribute());
        assert!(ValidatorState::None.can_finish());

        // Element -> can validate attribute, can end attributes
        assert!(ValidatorState::Element.can_validate_attribute());
        assert!(ValidatorState::Element.can_end_attributes());
        assert!(!ValidatorState::Element.can_validate_text());
        assert!(!ValidatorState::Element.can_end_element());

        // Attribute -> can continue attributes, can end attributes
        assert!(ValidatorState::Attribute.can_validate_attribute());
        assert!(ValidatorState::Attribute.can_end_attributes());

        // EndOfAttributes -> can have text, children, or end
        assert!(ValidatorState::EndOfAttributes.can_validate_text());
        assert!(ValidatorState::EndOfAttributes.can_start_element());
        assert!(ValidatorState::EndOfAttributes.can_end_element());

        // Text -> can have more text, children, or end
        assert!(ValidatorState::Text.can_validate_text());
        assert!(ValidatorState::Text.can_start_element());
        assert!(ValidatorState::Text.can_end_element());

        // EndElement -> can start sibling or end
        assert!(ValidatorState::EndElement.can_start_element());
        assert!(ValidatorState::EndElement.can_end_element());
        assert!(ValidatorState::EndElement.can_finish());

        // Finish -> nothing allowed
        assert!(!ValidatorState::Finish.can_start_element());
        assert!(!ValidatorState::Finish.can_validate_attribute());
        assert!(!ValidatorState::Finish.can_validate_text());
        assert!(!ValidatorState::Finish.can_end_element());
        assert!(!ValidatorState::Finish.can_finish());
    }
}
