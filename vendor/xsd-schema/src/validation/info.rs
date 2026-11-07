//! Validation output types returned to callers after each validation event
//!
//! These types represent the schema information associated with validated XML nodes.
//! `SchemaInfo` is the primary output, returned from each `validate_*` method.

use bitflags::bitflags;

use crate::ids::{AttributeKey, ElementKey, NameId, NotationKey, TypeKey};
use crate::types::value::XmlValue;

/// Validity status of a validated node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaValidity {
    /// Validity has not been determined
    #[default]
    NotKnown,
    /// The node is valid according to the schema
    Valid,
    /// The node is invalid according to the schema
    Invalid,
}

/// How much validation was attempted on a node (PSVI `[validation attempted]`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationAttempted {
    /// No validation was attempted
    #[default]
    None,
    /// Some but not all descendants were validated
    Partial,
    /// Full validation was performed on this node and all descendants
    Full,
}

/// Content type of a complex type, used to determine what children are allowed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentType {
    /// No child elements or text content allowed
    #[default]
    Empty,
    /// Text content only (simple content), possibly with attributes
    TextOnly,
    /// Child elements only, no interleaved text
    ElementOnly,
    /// Child elements with interleaved text allowed
    Mixed,
}

/// How the final `schema_type` was determined
///
/// `#[non_exhaustive]`: a variant can be added (one is feature-gated
/// already), so a `match` needs a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeSource {
    /// From the element/attribute declaration's resolved_type
    Declaration,
    /// Overridden by xsi:type attribute
    XsiType,
    /// Selected by Conditional Type Assignment (XSD 1.1)
    #[cfg(feature = "xsd11")]
    TypeAlternative,
    /// The type definition the run stipulated for the validation root
    /// ([`ValidationRoot::Type`], type-driven validation, Structures §5.2;
    /// §3.3.4.6 governing type definition clause "2 A type definition
    /// stipulated by the processor"). An `xsi:type` that overrides it is
    /// [`XsiType`](Self::XsiType).
    Stipulated,
}

/// Complex-type assertion evaluation outcome (XSD 1.1)
///
/// Covers only the buffered complex-type assertion path. Simple-type
/// assertion facet failures are reflected in `validity: Invalid` with
/// `cvc-assertion` constraint through the error sink.
#[cfg(feature = "xsd11")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionOutcome {
    /// All assertions evaluated and passed
    Passed,
    /// One or more assertions failed (includes compile/eval/EBV errors)
    Failed,
    /// Assertions exist but were not evaluated (PROCESS_ASSERTIONS not set,
    /// or evaluation deferred to an outer asserted element)
    NotEvaluated,
}

/// Stable node identity for cross-phase correlation (e.g., linking Phase 1 results to Phase 2 DOM nodes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIdentity(pub u64);

bitflags! {
    /// Flags controlling validation behavior
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ValidationFlags: u32 {
        /// Report warnings in addition to errors (default on).
        const REPORT_WARNINGS = 0x0001;
        /// Has no effect: identity constraints (`xs:key`, `xs:unique`,
        /// `xs:keyref`) are always enforced.
        ///
        /// They are part of an element's validity — Structures §3.3.4.3
        /// Element Locally Valid (Element), clause 6: "E is ·valid· with
        /// respect to each of the {identity-constraint definitions} as per
        /// Identity-constraint Satisfied (§3.11.4)" — and the runtime has
        /// always enforced them whether or not this bit was set; their cost
        /// is paid only by elements in the scope of a constraint. Setting or
        /// clearing the bit changes nothing; remove it from your flags.
        #[deprecated(
            since = "0.3.0",
            note = "identity constraints are always enforced; the flag has no effect"
        )]
        const PROCESS_IDENTITY_CONSTRAINTS = 0x0002;
        /// Accept every attribute in the reserved
        /// `http://www.w3.org/XML/1998/namespace` namespace (i.e. `xml:lang`,
        /// `xml:space`, `xml:base`, `xml:id`) without checking the element's
        /// complex type for an allowing declaration or wildcard.
        ///
        /// This bit is a lenient-parser convenience, **not** an XSD
        /// conformance mode. The XSD 1.0/1.1 spec requires every attribute
        /// — including those in the xml namespace — to be matched by a
        /// declared `{attribute use}` or an `{attribute wildcard}` whose
        /// namespace constraint admits the xml namespace. Enabling this
        /// flag therefore deviates from strict conformance; it is off by
        /// default.
        ///
        /// Typical use: set this bit when feeding arbitrary XML through the
        /// validator and you want `xml:lang` to "just work" even against
        /// schemas that don't explicitly import the xml namespace. Leave
        /// it clear for strict XSD conformance validation (including the
        /// W3C XSD test suite).
        const ALLOW_XML_ATTRIBUTES = 0x0004;
        /// Strict mode: treat all warnings as errors.
        const STRICT_MODE = 0x0008;
        /// Enable XSD 1.1 assertion processing (fragment buffering).
        ///
        /// Honoured by `SchemaValidator::new`, which then evaluates
        /// assertions inline while streaming, as
        /// `SchemaValidator::new_fragment_buffer` does (that constructor sets
        /// the bit itself); `SchemaValidator::new_main_document` removes it.
        #[cfg(feature = "xsd11")]
        const PROCESS_ASSERTIONS = 0x0010;
        /// Retain the typed value (`SchemaInfo::typed_value`) and the schema
        /// normalized value (`SchemaInfo::normalized_value`) for caller
        /// consumption (PSVI). **On by default.**
        ///
        /// Clear this bit in streaming mode when you only need validity and
        /// errors. The runtime still materializes a typed value *internally*
        /// wherever validation requires it — a value-space facet (numeric
        /// range, `totalDigits`, value-space `enumeration`), a `fixed`/`default`
        /// comparison, an ID/IDREF/ENTITY or QName/NOTATION type, an active
        /// identity constraint, or an XSD 1.1 assertion — so validity is
        /// unaffected. The only observable change is that `typed_value` /
        /// `normalized_value` may be `None` on the returned `SchemaInfo` for
        /// nodes whose value nothing else needed. See `PERF_LAZY_PSVI.md`.
        ///
        /// `SchemaInfo::typed_value` is kept, whatever this bit says, for an
        /// element that took its declaration's `default` / `fixed` value and
        /// for elements and attributes whose type is or can hold `xs:ID`,
        /// `xs:IDREF`, `xs:QName` or `xs:NOTATION` values (derived, list and
        /// union types included): see [`SchemaInfo::typed_value`].
        const BUILD_PSVI_TYPED_VALUES = 0x0020;
        /// Put the types the XPath 2.0 data model adds to the XML Schema
        /// namespace (XDM 1.0 §2.6.2) in scope for `xsi:type` where the set's
        /// XSD version does not define them: in an XSD 1.0 set
        /// `xs:anyAtomicType`, `xs:untypedAtomic`, `xs:dayTimeDuration` and
        /// `xs:yearMonthDuration`, in an XSD 1.1 set `xs:untypedAtomic`
        /// (Structures 1.1 §1.3.1.1: not "defined in this specification").
        /// An `xsi:type` naming one then resolves to the set's component
        /// ([`SchemaSet::xpath_type`](crate::SchemaSet::xpath_type)) and
        /// overrides the governing type when ·validly substitutable· for it,
        /// by the types' places in the data model's hierarchy.
        ///
        /// For a host that validates nodes of the data model, whose in-scope
        /// schema types include them. Off by default: an XML Schema processor
        /// does not resolve them (`cvc-elt.4.1`). Schema documents are not
        /// affected — they name only what their XSD version defines.
        const XPATH_TYPES = 0x0040;
    }
}

impl Default for ValidationFlags {
    /// The strict-conformance defaults.
    ///
    /// The default enables `REPORT_WARNINGS` and `BUILD_PSVI_TYPED_VALUES`
    /// (PSVI typed/normalized values are retained for back-compat).
    /// `xml:*` leniency, strict-mode warning promotion, and XSD 1.1 assertion
    /// processing are opt-in — combine them with `|`; PSVI retention is
    /// opt-*out* — remove it with `& !`. Identity constraints are always
    /// enforced (the `PROCESS_IDENTITY_CONSTRAINTS` bit is deprecated and has
    /// no effect):
    ///
    /// ```
    /// use xsd_schema::validation::ValidationFlags;
    ///
    /// let flags = ValidationFlags::default() | ValidationFlags::ALLOW_XML_ATTRIBUTES;
    /// // Opt out of PSVI value retention for faster streaming:
    /// let lean = ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES;
    /// # let _ = (flags, lean);
    /// ```
    fn default() -> Self {
        ValidationFlags::REPORT_WARNINGS | ValidationFlags::BUILD_PSVI_TYPED_VALUES
    }
}

/// How a run assesses its validation root — the element its first event
/// opens (Structures §5.2 "Assessing Schema-Validity"). Set with
/// [`ValidationRuntime::set_validation_root`](super::ValidationRuntime::set_validation_root)
/// before the root's element event; the default is
/// [`Declaration`](Self::Declaration), the behaviour of every run that does
/// not set one.
///
/// Every driver that takes the caller's runtime honours it —
/// `drive_quick_xml` and its variants, `drive_navigator`,
/// `drive_buffer_document`, and the typed build `build_typed_document_with`
/// (whose `TypedBuilderHandler` takes each element's binding from the
/// [`SchemaInfo`] the mode produces). `build_typed_document`, which makes its
/// own runtime, always uses the default.
///
/// The mode applies to each validation root of the run: every element
/// started while no element is open. Only the root is affected; its
/// attributes and children are assessed as Structures §3.3.4.6 prescribes
/// for the root's governing declaration or type. The checks that need the
/// whole run — Validation Root Valid (ID/IDREF) (§3.3.4.5) and identity
/// constraints — apply in every mode.
///
/// | Mode | Root's `element_decl` | `schema_type` | `[validity]`, `[validation attempted]` | Errors of its own |
/// |---|---|---|---|---|
/// | `Declaration` | the top-level declaration of its name | the declaration's type (or `xsi:type`, a type alternative) | strict assessment | `cvc-elt.1` when undeclared and without `xsi:type` |
/// | `Element(D)` | `D` | `D`'s type (or `xsi:type`, a type alternative) | strict assessment | `cvc-elt.1` when the root's name is not `D`'s |
/// | `Type(T)` | none | `T`, or an `xsi:type` that overrides it | strict assessment | `cvc-elt.4.*` for an `xsi:type` that does not override `T`, `cvc-type.2` for an abstract governing type |
/// | `Lax` | as `Declaration` | as `Declaration` | strict when declared or `xsi:type` names a type; otherwise *notKnown*, and *none* / *partial* | none for an undeclared root |
///
/// `#[non_exhaustive]`, so further modes can be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ValidationRoot {
    /// *Strict wildcard validation* (§5.2): "The processor starts from
    /// Schema-Validity Assessment (Element) (§3.3.4.6) with no stipulated
    /// declaration or definition. If the ·validation root· and the schema
    /// determine an element declaration (by the name of the element) … or a
    /// type definition (via xsi:type), then ·strict· validation is
    /// performed"; otherwise the root is laxly assessed and its absence from
    /// the schema is reported as `cvc-elt.1` — the case §5.2 describes as
    /// "the invoking process expects the ·validation root· to be declared and
    /// valid and will otherwise report an error to its environment". The
    /// behaviour of a run that sets no mode.
    #[default]
    Declaration,
    /// *Element-driven validation* (§5.2): "the item is validated as described
    /// in Schema-Validity Assessment (Element) (§3.3.4.6) (with the stipulated
    /// element declaration as the ·governing· declaration)" — §3.3.4.6,
    /// governing element declaration, clause "1 A declaration stipulated by
    /// the processor". The declaration governs whatever the root's name;
    /// Element Locally Valid (Element) (§3.3.4.3) "1 D is not ·absent· and E
    /// and D have the same expanded name" makes a root of another name
    /// invalid (`cvc-elt.1`; XSD 1.0 §5.2: the application "checks that its
    /// {name} and {target namespace} match"). Everything else — `xsi:type`,
    /// `xsi:nil`, `{abstract}`, type alternatives, identity constraints — is
    /// as for a declared element.
    Element(crate::ids::ElementKey),
    /// *Type-driven validation* (§5.2): "If the ·validation root· is an
    /// element, then it is validated as described in Schema-Validity
    /// Assessment (Element) (§3.3.4.6) (with the stipulated type definition
    /// as the ·governing type definition·)". The root has no governing
    /// element declaration, even when a top-level declaration of its name
    /// exists (§3.3.4.6, governing element declaration clause 4.2: none is
    /// resolved by name when "the processor has stipulated a type definition
    /// for E").
    ///
    /// - **`xsi:type`**: the governing type is "1 An ·instance-specified type
    ///   definition· which ·overrides· a type definition stipulated by the
    ///   processor", else "2 A type definition stipulated by the processor".
    ///   It overrides when ·validly substitutable· for the stipulated type
    ///   "subject to the blocking keywords in the union of K and
    ///   T.{prohibited substitutions}" with K empty (§3.3.4.2; XSD 1.0 §3.3.4
    ///   clause 1.2.1.2.4: "validly derived from that type definition given
    ///   its {prohibited substitutions}"). One that does not is reported as
    ///   for a declared element (`cvc-elt.4.1` unresolved, `cvc-elt.4.2` not
    ///   derived, `cvc-elt.4.3` blocked), the root is invalid, and the
    ///   stipulated type stays governing.
    /// - **Abstract**: an abstract governing complex type is `cvc-type.2`
    ///   (Element Locally Valid (Type) §3.3.4.4 clause 2); an `xsi:type` that
    ///   overrides it with a concrete type is valid.
    /// - **`xsi:nil`**: ·nilled· is defined "with respect to some element
    ///   declaration D" with "D.{nillable} = true" (§3.3.1), so the root is
    ///   never nilled: `xsi:nil` is checked as an `xs:boolean` attribute and
    ///   the content against the type.
    /// - **PSVI**: `element_decl` `None`, `schema_type` the governing type,
    ///   `type_source` [`TypeSource::Stipulated`] for the stipulated type and
    ///   [`TypeSource::XsiType`] for an overriding `xsi:type`; strictly
    ///   assessed (§3.3.4.6 clause 1.2), so `[validity]` is valid or invalid.
    /// - A simple type is allowed (§3.3.4.4 clause 3.1: no attributes but
    ///   `xsi:*`, no element children, the text String Valid).
    Type(crate::ids::TypeKey),
    /// *Lax wildcard validation* (§5.2): as [`Declaration`](Self::Declaration)
    /// when the root's name resolves to a top-level declaration or its
    /// `xsi:type` to a type — "then ·strict validation· is performed. If they
    /// do not identify any declaration or definition, then ·lax assessment·
    /// is performed" ("analogous to the validation of an element information
    /// item which matches a lax wildcard"): "E is locally ·validated· with
    /// respect to ·xs:anyType·" and the schema-validity of "E's \[attributes\]
    /// and \[children\] is assessed" (§3.3.4.6) — a declared child or a
    /// globally declared attribute strictly, and its errors reported.
    ///
    /// A laxly assessed root has no type and no declaration, its
    /// `[validity]` is *notKnown* whatever its attributes and children
    /// (§3.3.5.1 clause 2: "2 otherwise notKnown"), and its `[validation
    /// attempted]` *none*, or *partial* when an attribute or child was
    /// assessed. No `cvc-elt.1` is reported. As for an element a lax wildcard
    /// matched, an attribute of its own that its global declaration finds
    /// invalid, and an `xsi:type` that names no type (`cvc-elt.4.1`; no
    /// ·instance-specified type definition·, §3.3.4.1), are reported, and the
    /// root stays notKnown. The root is ·nilled· only with a declaration
    /// (§3.3.1): with an `xsi:type` alone, `xsi:nil` is an attribute and the
    /// content is checked against the type.
    ///
    /// A host that wants an `xsi:type` attribute alone not to cause strict
    /// validation passes no `xsi:type` to the element event.
    Lax,
}

/// Content processing mode for wildcard-matched elements/attributes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentProcessing {
    /// Must be validated against the schema; error if no declaration found
    #[default]
    Strict,
    /// Validate if declaration found; skip if not
    Lax,
    /// Do not validate content
    Skip,
}

/// Schema information returned after validating a node
///
/// Contains the resolved schema components and validation status for an element or attribute.
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    /// The element declaration, if one was found
    pub element_decl: Option<ElementKey>,
    /// The attribute declaration, if one was found
    pub attribute_decl: Option<AttributeKey>,
    /// The resolved schema type (simple or complex)
    pub schema_type: Option<TypeKey>,
    /// For union types: the actual member type that matched the value
    pub member_type: Option<TypeKey>,
    /// Validity status
    pub validity: SchemaValidity,
    /// How much validation was attempted (PSVI `[validation attempted]`)
    pub validation_attempted: ValidationAttempted,
    /// Whether the value was supplied by the declaration's value constraint
    /// — an element with no element or character \[children\] that took its
    /// `default` or `fixed` value (clause 5.1 of Element Locally Valid
    /// (Element), Structures §3.3.4.3; PSVI \[schema specified\] "schema",
    /// §3.3.5.5)
    pub is_default: bool,
    /// For an element's start: whether the element is ·nilled· (Structures
    /// §3.3.1: "1 E has xsi:nil = true. 2 D.{nillable} = true", with respect
    /// to its governing element declaration D) — `xsi:nil="true"` on an
    /// element whose declaration is nillable. `false` without a declaration
    /// and on a non-nillable one (where `xsi:nil` is `cvc-elt.3.1`).
    ///
    /// For an element's end (`validate_end_element`): its \[nil\] (§3.3.5.3:
    /// "true if clause 3.2.3 of Element Locally Valid (Element) … is
    /// satisfied, otherwise false"), so also `false` for a nilled element
    /// that has character or element \[children\] (clause 3.2.3.1; the
    /// element is invalid, `cvc-elt.3.2.1`) or whose declaration has a fixed
    /// value (clause 3.2.3.2; invalid, `cvc-elt.3.2.2`).
    pub is_nil: bool,
    /// Content type of the element (Empty, TextOnly, ElementOnly, Mixed)
    pub content_type: Option<ContentType>,
    /// The typed value from simple-type validation — the node's \[schema
    /// actual value\] (Structures §3.3.5.4, §3.2.5.2). `None` when it has none:
    /// no simple type or simple content, a value that is not valid, a
    /// ·nilled· element; and for an element's start and end of attributes.
    /// An `xs:QName` / `xs:NOTATION` in it holds its expanded name, read with
    /// the bindings in scope at the node — or, for an element that took its
    /// declaration's value constraint, with those of the schema element the
    /// value is written on (the {value}; see
    /// [`SchemaSet::element_value_constraint_for_type`](crate::SchemaSet::element_value_constraint_for_type),
    /// which gives it before the element's content).
    ///
    /// **Kept without PSVI retention.** With
    /// [`ValidationFlags::BUILD_PSVI_TYPED_VALUES`] cleared the runtime may
    /// leave this `None` for a node whose value nothing needed, but never for
    /// the following — for them it is what it is with the flag set:
    ///
    /// - an element that took its declaration's `default` / `fixed` value
    ///   ([`is_default`](Self::is_default); §3.3.4.3 clause 5.1), whatever its
    ///   type;
    /// - an element or attribute whose type's values are or can hold `xs:ID`,
    ///   `xs:IDREF`, `xs:QName` or `xs:NOTATION` values: a type derived from
    ///   one of them, a list type with such items (`xs:IDREFS` included), a
    ///   union with such a member type, a complex type with such simple
    ///   content.
    ///
    /// That holds wherever the runtime hands out a `SchemaInfo` for an
    /// element's end or an attribute: the push API
    /// ([`validate_end_element`](crate::validation::ValidationRuntime::validate_end_element),
    /// [`validate_attribute`](crate::validation::ValidationRuntime::validate_attribute),
    /// and in XSD 1.1 the results of `take_deferred_attribute_results`) and
    /// the
    /// [`after_attribute`](crate::validation::ValidationEventHandler::after_attribute)
    /// hook of the streaming drivers.
    pub typed_value: Option<XmlValue>,
    /// The whitespace-normalized value (PSVI `[schema normalized value]`).
    /// For an element that took its declaration's value constraint
    /// ([`is_default`](Self::is_default)), the constraint's {lexical form}
    /// (Structures §3.3.5.4 clause 1.1), also under an `xsi:type` or a type
    /// alternative — see
    /// [`ElementValueConstraint::lexical_form`](crate::schema::ElementValueConstraint::lexical_form).
    pub normalized_value: Option<String>,
    /// Constraint codes from validation errors on this node (PSVI `[schema error code]`)
    pub schema_error_codes: Vec<&'static str>,
    /// Notation declaration resolved from a NOTATION-typed attribute (PSVI `[notation]`).
    /// Only meaningful on element-end SchemaInfo; always `None` for attributes.
    pub notation: Option<NotationKey>,
    /// Whether this attribute was deferred due to CTA (type alternatives)
    pub deferred_by_cta: bool,
    /// How the `schema_type` was determined (declaration, xsi:type, or CTA)
    pub type_source: Option<TypeSource>,
    /// Whether CTA evaluation selected a type (even if it matches the declared type)
    #[cfg(feature = "xsd11")]
    pub cta_selected: bool,
    /// Complex-type assertion outcome (XSD 1.1, end-element SchemaInfo only)
    #[cfg(feature = "xsd11")]
    pub assertion_outcome: Option<AssertionOutcome>,
}

impl SchemaInfo {
    /// Create a SchemaInfo with all fields set to None/default
    pub fn empty() -> Self {
        SchemaInfo {
            element_decl: None,
            attribute_decl: None,
            schema_type: None,
            member_type: None,
            validity: SchemaValidity::NotKnown,
            validation_attempted: ValidationAttempted::None,
            is_default: false,
            is_nil: false,
            content_type: None,
            typed_value: None,
            normalized_value: None,
            schema_error_codes: Vec::new(),
            notation: None,
            deferred_by_cta: false,
            type_source: None,
            #[cfg(feature = "xsd11")]
            cta_selected: false,
            #[cfg(feature = "xsd11")]
            assertion_outcome: None,
        }
    }

    /// Create a SchemaInfo indicating a valid element
    pub fn valid_element(
        element_decl: ElementKey,
        schema_type: TypeKey,
        content_type: ContentType,
    ) -> Self {
        SchemaInfo {
            element_decl: Some(element_decl),
            attribute_decl: None,
            schema_type: Some(schema_type),
            member_type: None,
            validity: SchemaValidity::Valid,
            validation_attempted: ValidationAttempted::Full,
            is_default: false,
            is_nil: false,
            content_type: Some(content_type),
            typed_value: None,
            normalized_value: None,
            schema_error_codes: Vec::new(),
            notation: None,
            deferred_by_cta: false,
            type_source: Some(TypeSource::Declaration),
            #[cfg(feature = "xsd11")]
            cta_selected: false,
            #[cfg(feature = "xsd11")]
            assertion_outcome: None,
        }
    }

    /// Create a SchemaInfo indicating a valid attribute
    pub fn valid_attribute(attribute_decl: AttributeKey, schema_type: TypeKey) -> Self {
        SchemaInfo {
            element_decl: None,
            attribute_decl: Some(attribute_decl),
            schema_type: Some(schema_type),
            member_type: None,
            validity: SchemaValidity::Valid,
            validation_attempted: ValidationAttempted::Full,
            is_default: false,
            is_nil: false,
            content_type: None,
            typed_value: None,
            normalized_value: None,
            schema_error_codes: Vec::new(),
            notation: None,
            deferred_by_cta: false,
            type_source: Some(TypeSource::Declaration),
            #[cfg(feature = "xsd11")]
            cta_selected: false,
            #[cfg(feature = "xsd11")]
            assertion_outcome: None,
        }
    }

    /// Create a SchemaInfo with Invalid validity
    pub fn invalid() -> Self {
        SchemaInfo {
            validity: SchemaValidity::Invalid,
            ..SchemaInfo::empty()
        }
    }

    /// Returns `true` if the resolved schema type is a simple type.
    pub fn is_simple_type(&self) -> bool {
        matches!(self.schema_type, Some(TypeKey::Simple(_)))
    }

    /// Returns `true` if the resolved schema type is a complex type.
    pub fn is_complex_type(&self) -> bool {
        matches!(self.schema_type, Some(TypeKey::Complex(_)))
    }
}

/// An expected element in the current content model position
#[derive(Debug, Clone)]
pub struct ExpectedElement {
    /// Local name of the expected element
    pub local_name: NameId,
    /// Namespace of the expected element
    pub namespace: Option<NameId>,
    /// The element declaration key, if available
    pub element_key: Option<ElementKey>,
}

/// An expected attribute for the current element type
#[derive(Debug, Clone)]
pub struct ExpectedAttribute {
    /// Local name of the attribute
    pub local_name: NameId,
    /// Namespace of the attribute
    pub namespace: Option<NameId>,
    /// The attribute declaration key
    pub attribute_key: Option<AttributeKey>,
    /// Whether the attribute is required
    pub required: bool,
}

/// A defaulted attribute of the current element (Structures §3.4.4.2, added to
/// the element's \[attributes\] by §3.4.5.1 *Attribute Default Value*): an
/// optional attribute use of the element's complex type — its own, from an
/// attribute group, or inherited from its base type — with a `default` or
/// `fixed` value constraint, which the element does not carry.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DefaultAttribute {
    /// Local name of the attribute
    pub local_name: NameId,
    /// Namespace of the attribute
    pub namespace: Option<NameId>,
    /// The global attribute declaration, `None` for a local one (an
    /// `<xs:attribute name="…">` inside a complex type or an attribute group)
    pub attribute_key: Option<AttributeKey>,
    /// The declaration's type definition; `None` when it names none
    /// (`xs:anySimpleType`)
    pub type_key: Option<TypeKey>,
    /// The ·effective value constraint·'s lexical form: the use's `default` /
    /// `fixed` value, otherwise the declaration's
    pub value: String,
    /// The ·effective value constraint·'s {value} — the defaulted attribute's
    /// \[schema actual value\] (Structures §3.4.5.1) — read with the namespace
    /// bindings of the schema element the value is written on (Datatypes
    /// §3.3.18), so that an `xs:QName` / `xs:NOTATION` value has the schema's
    /// expanded name, whatever the instance binds its prefix to. Present when
    /// the run retains PSVI typed values
    /// ([`ValidationFlags::BUILD_PSVI_TYPED_VALUES`]), and always for a type
    /// whose values can be or hold QNames or NOTATIONs; `None` otherwise, or
    /// when the value is not valid for the type.
    pub typed_value: Option<XmlValue>,
}

/// An inherited attribute from an ancestor element (XSD 1.1 §3.3.5.6).
///
/// Represents an entry in the PSVI `[inherited attributes]` property.
#[cfg(feature = "xsd11")]
#[derive(Debug, Clone)]
pub struct InheritedAttribute {
    /// Local name of the attribute
    pub local_name: NameId,
    /// Namespace of the attribute
    pub namespace: Option<NameId>,
    /// The governing attribute declaration key, if known
    pub attribute_key: Option<AttributeKey>,
    /// The inherited attribute value
    pub value: String,
}

/// A schema location hint extracted from `xsi:schemaLocation`.
///
/// Pairs a namespace URI with a schema location URI plus the base URI
/// of the instance document element where the hint was found (needed to
/// resolve relative location URIs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaLocationHint {
    /// The namespace URI (first token of each pair in `xsi:schemaLocation`).
    pub namespace: String,
    /// The schema location URI (second token of each pair).
    pub location: String,
    /// Base URI of the instance document at the point where this hint was
    /// found. Empty if no base URI was set on the runtime.
    pub base_uri: String,
}

/// A no-namespace schema location hint extracted from
/// `xsi:noNamespaceSchemaLocation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoNamespaceSchemaLocationHint {
    /// The schema location URI.
    pub location: String,
    /// Base URI of the instance document at the point where this hint was
    /// found. Empty if no base URI was set on the runtime.
    pub base_uri: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_info_empty() {
        let info = SchemaInfo::empty();
        assert_eq!(info.validity, SchemaValidity::NotKnown);
        assert!(info.element_decl.is_none());
        assert!(info.attribute_decl.is_none());
        assert!(info.schema_type.is_none());
        assert!(info.member_type.is_none());
        assert!(!info.is_default);
        assert!(!info.is_nil);
        assert!(info.content_type.is_none());
        assert!(info.typed_value.is_none());
        assert!(info.type_source.is_none());
        #[cfg(feature = "xsd11")]
        {
            assert!(!info.cta_selected);
            assert!(info.assertion_outcome.is_none());
        }
    }

    #[test]
    fn test_schema_info_invalid() {
        let info = SchemaInfo::invalid();
        assert_eq!(info.validity, SchemaValidity::Invalid);
        assert!(info.element_decl.is_none());
    }

    #[test]
    fn test_is_simple_type() {
        let info = SchemaInfo::empty();
        assert!(!info.is_simple_type());
        assert!(!info.is_complex_type());

        use slotmap::SlotMap;
        let mut sm: SlotMap<crate::ids::SimpleTypeKey, ()> = SlotMap::with_key();
        let sk = sm.insert(());
        let mut info = SchemaInfo::empty();
        info.schema_type = Some(TypeKey::Simple(sk));
        assert!(info.is_simple_type());
        assert!(!info.is_complex_type());
    }

    #[test]
    fn test_is_complex_type() {
        use slotmap::SlotMap;
        let mut sm: SlotMap<crate::ids::ComplexTypeKey, ()> = SlotMap::with_key();
        let ck = sm.insert(());
        let mut info = SchemaInfo::empty();
        info.schema_type = Some(TypeKey::Complex(ck));
        assert!(info.is_complex_type());
        assert!(!info.is_simple_type());
    }

    #[test]
    fn test_schema_validity_default() {
        let v = SchemaValidity::default();
        assert_eq!(v, SchemaValidity::NotKnown);
    }

    #[test]
    fn test_content_type_default() {
        let ct = ContentType::default();
        assert_eq!(ct, ContentType::Empty);
    }

    #[test]
    fn test_content_processing_default() {
        let cp = ContentProcessing::default();
        assert_eq!(cp, ContentProcessing::Strict);
    }

    #[test]
    fn test_validation_flags_default() {
        let flags = ValidationFlags::default();
        assert!(flags.contains(ValidationFlags::REPORT_WARNINGS));
        // PSVI typed/normalized values are retained by default (opt-out).
        assert!(flags.contains(ValidationFlags::BUILD_PSVI_TYPED_VALUES));
        // Strict-conformance defaults: ALLOW_XML_ATTRIBUTES and strict-mode
        // warning promotion are opt-in. (Identity constraints are always
        // enforced; their deprecated bit stays out of the default value.)
        assert!(!flags.contains(ValidationFlags::ALLOW_XML_ATTRIBUTES));
        #[allow(deprecated)]
        let ic = ValidationFlags::PROCESS_IDENTITY_CONSTRAINTS;
        assert!(!flags.contains(ic));
        assert!(!flags.contains(ValidationFlags::STRICT_MODE));
    }

    #[test]
    fn test_validation_flags_bitops() {
        let flags = ValidationFlags::REPORT_WARNINGS | ValidationFlags::STRICT_MODE;
        assert!(flags.contains(ValidationFlags::REPORT_WARNINGS));
        assert!(flags.contains(ValidationFlags::STRICT_MODE));
        assert!(!flags.contains(ValidationFlags::ALLOW_XML_ATTRIBUTES));

        let combined = flags | ValidationFlags::ALLOW_XML_ATTRIBUTES;
        assert!(combined.contains(ValidationFlags::ALLOW_XML_ATTRIBUTES));
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn test_process_assertions_flag() {
        let default_flags = ValidationFlags::default();
        assert!(
            !default_flags.contains(ValidationFlags::PROCESS_ASSERTIONS),
            "PROCESS_ASSERTIONS must not be in defaults"
        );
        let with_flag = default_flags | ValidationFlags::PROCESS_ASSERTIONS;
        assert!(with_flag.contains(ValidationFlags::PROCESS_ASSERTIONS));
        // Original defaults still present
        assert!(with_flag.contains(ValidationFlags::REPORT_WARNINGS));
        // ALLOW_XML_ATTRIBUTES is no longer in the default — verify it stays
        // opt-in when callers explicitly mix it in.
        assert!(!with_flag.contains(ValidationFlags::ALLOW_XML_ATTRIBUTES));
        let with_xml = with_flag | ValidationFlags::ALLOW_XML_ATTRIBUTES;
        assert!(with_xml.contains(ValidationFlags::ALLOW_XML_ATTRIBUTES));
    }

    #[test]
    fn test_node_identity_eq() {
        let a = NodeIdentity(42);
        let b = NodeIdentity(42);
        let c = NodeIdentity(99);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
