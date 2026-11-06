//! An element declaration's {value constraint}, for hosts that construct and
//! validate elements themselves.
//!
//! Structures §3.3.1 gives an element declaration the property "{value
//! constraint} A Value Constraint property record. Optional.", whose record is
//! "{variety} One of {default, fixed}", "{value} An ·actual value·" and
//! "{lexical form} A character string". §3.3.2.1 maps the `default` / `fixed`
//! attribute of `<xs:element>` to it: "\[Definition:\] Use the name effective
//! simple type definition for the declared {type definition}, if it is a
//! simple type definition, or, if {type definition}.{content type}.{variety} =
//! simple, for {type definition}.{content type}.{simple type definition}, or
//! else for the built-in string simple type definition)" — {variety} "either
//! default or fixed, as appropriate", {value} "the ·actual value· (with
//! respect to the ·effective simple type definition·) of the \[attribute\]",
//! {lexical form} "the ·normalized value· (with respect to the ·effective
//! simple type definition·) of the \[attribute\]".
//!
//! An `xs:QName` or `xs:NOTATION` in the value is read with the bindings of the
//! schema element that carries the attribute (Datatypes §3.3.18: the lexical
//! mapping "depends on the namespace bindings in scope where the literal
//! occurs. When QNames appear in an XML context, the bindings to be used in the
//! ·lexical mapping· are those in the \[in-scope namespaces\] property of the
//! relevant element"; §3.3.19: "The lexical mapping rules for NOTATION are as
//! given for QName"), so its namespace is the one the schema means, whatever an
//! instance binds the prefix to.
//!
//! A host that builds an element and validates it through the push API learns
//! the governing declaration and type before any content:
//! [`validate_element`](crate::validation::ValidationRuntime::validate_element)
//! names them ([`SchemaInfo::element_decl`](crate::validation::SchemaInfo::element_decl),
//! [`SchemaInfo::schema_type`](crate::validation::SchemaInfo::schema_type), an
//! `xsi:type` applied), and in XSD 1.1
//! [`validate_end_of_attributes`](crate::validation::ValidationRuntime::validate_end_of_attributes)
//! names the type a type alternative selected (its `schema_type` is set only
//! then). With [`SchemaSet::element_value_constraint_for_type`] the host can
//! then declare, on the start tag it writes, a prefix for each namespace a
//! QName of the value needs — the value the element takes if it ends with
//! neither element nor character children (§3.3.4.3 clause 5.1) — and hand
//! that start tag, with the declaration in its namespace snapshot, to
//! whatever reads the element next.
//!
//! Under a governing type other than the declared one, Structures §3.3.4.3
//! clause 5.1 reads: "5.1.1 If E's ·governing type definition· is an
//! ·instance-specified type definition·, then D.{value constraint} is a valid
//! default for the ·governing type definition· as defined in Element Default
//! Valid (Immediate) (§3.3.6.2). 5.1.2 The element information item with
//! D.{value constraint}.{lexical form} used as its ·normalized value· is
//! locally ·valid· with respect to the ·governing type definition· as defined
//! by Element Locally Valid (Type) (§3.3.4.4)."
//! [`ElementValueConstraint::typed_value`] answers 5.1.2,
//! [`ElementValueConstraint::valid_default`] answers 5.1.1.

use crate::arenas::ElementDeclData;
use crate::ids::{ElementKey, TypeKey};
use crate::schema::SchemaSet;
use crate::types::value::XmlValue;
use crate::types::XmlTypeCode;
use crate::validation::info::ContentType;

/// The {variety} of a Value Constraint (Structures §3.3.1: "One of {default,
/// fixed}").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueConstraintVariety {
    /// `default`: an element with neither element nor character children
    /// takes the value (§3.3.4.3 clause 5.1).
    Default,
    /// `fixed`: as `default` for such an element; otherwise the element's
    /// value must be equal or identical to the {value} (clause 5.2.2).
    Fixed,
}

/// An element declaration's {value constraint}, read under its own type
/// ([`SchemaSet::element_value_constraint`]) or under a governing type
/// ([`SchemaSet::element_value_constraint_for_type`]).
///
/// `#[non_exhaustive]`, so further properties can be added.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ElementValueConstraint {
    /// {variety}: whether the declaration's attribute is `default` or `fixed`.
    pub variety: ValueConstraintVariety,
    /// {lexical form}: the attribute's ·normalized value· with respect to the
    /// declaration's ·effective simple type definition· — white space
    /// normalized as that type normalizes it (a union's as the member that
    /// accepts it does), a QName with the prefix the schema wrote. The same
    /// under every governing type: when the element takes the value
    /// constraint it is its ·normalized value· (§3.3.4.3 clause 5.1.2) and its
    /// \[schema normalized value\] (§3.3.5.4 clause 1.1: "If clause 5.1 of
    /// Element Locally Valid (Element) (§3.3.4.3) above has applied, then the
    /// {lexical form} of the {value constraint}").
    pub lexical_form: String,
    /// The value, as an [`XmlValue`] like
    /// [`SchemaInfo::typed_value`](crate::validation::SchemaInfo::typed_value);
    /// each `xs:QName` / `xs:NOTATION` in it (atomic, a list member, a union's
    /// member value) holds the expanded name read with the bindings of the
    /// schema element that carries the attribute, with the schema's prefix.
    ///
    /// - From [`SchemaSet::element_value_constraint`]: the {value} — the
    ///   ·actual value· with respect to the declaration's ·effective simple
    ///   type definition· (`xs:string` for a mixed content type).
    /// - From [`SchemaSet::element_value_constraint_for_type`]: the
    ///   ·actual value· of the {lexical form} under the governing type —
    ///   clause 5.1.2, the element's \[schema actual value\]. `None` when the
    ///   governing type's content is not simple (an element of it has no
    ///   \[schema actual value\], §3.3.5.4 clause 2).
    ///
    /// `None` too when the form is not valid for the type, or holds a QName /
    /// NOTATION whose prefix the schema element does not bind.
    pub typed_value: Option<XmlValue>,
    /// Whether the value constraint is a valid default for the type it was
    /// read under — Element Default Valid (Immediate) (§3.3.6.2), what clause
    /// 5.1.1 requires of an ·instance-specified type definition· (`xsi:type`).
    ///
    /// - A simple type, or simple content: Simple Default Valid (§3.2.6.2): "1
    ///   V.{lexical form} is ·valid· with respect to T … 2 V.{lexical form}
    ///   maps to V.{value} in the value space of T" — `typed_value` is
    ///   present and equal or identical to the declaration's {value} (not
    ///   compared when the declared type has no value space of its own:
    ///   `xs:anySimpleType`).
    /// - Other content: "2.1 T.{content type}.{variety} = mixed. 2.2 The
    ///   particle T.{content type}.{particle} is ·emptiable·".
    ///
    /// An empty element whose governing type an `xsi:type` gave is invalid
    /// when this is `false` (`cvc-elt.5.1.1`). For the declared type, or a type
    /// an XSD 1.1 type alternative selected (not ·instance-specified·), only
    /// clause 5.1.2 applies: with simple content the element is invalid when
    /// `typed_value` is `None`; with other content its content model decides
    /// (it has no children).
    pub valid_default: bool,
}

impl SchemaSet {
    /// The {value constraint} of the element declaration `element` (Structures
    /// §3.3.1, mapped by §3.3.2.1): its {variety}, its {lexical form}, and its
    /// {value} — the ·actual value· with respect to the declaration's
    /// ·effective simple type definition·, a QName / NOTATION read with the
    /// bindings of the schema element the value is written on —, and whether
    /// it is a valid default for the declared {type definition}. `None` when
    /// the declaration has no `default` and no `fixed` value, or when
    /// `element` is not a declaration of this set.
    ///
    /// Works for top-level and local declarations (a reference entry reads
    /// the declaration it resolves to) in both XML Schema versions. The value
    /// is the one validation applies when the declaration's {type definition}
    /// governs the element and its content is simple: an element with neither
    /// element nor character children that takes it reports it as
    /// [`SchemaInfo::typed_value`](crate::validation::SchemaInfo::typed_value)
    /// at its end, whatever the instance binds the value's prefixes to. When
    /// another type governs — an `xsi:type`, an XSD 1.1 type alternative — use
    /// [`element_value_constraint_for_type`](Self::element_value_constraint_for_type).
    ///
    /// The effective simple type definition is the declared type when it is
    /// simple, its {content type}'s simple type definition when its content is
    /// simple, else `xs:string` (a mixed content type; its element has no
    /// typed value, §3.3.5.4 clause 2).
    ///
    /// ```
    /// use xsd_schema::schema::ValueConstraintVariety;
    /// use xsd_schema::types::value::{XmlAtomicValue, XmlValueKind};
    /// use xsd_schema::SchemaSetBuilder;
    ///
    /// let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
    ///     xmlns:p="urn:p" targetNamespace="urn:t">
    ///   <xs:element name="q" type="xs:QName" default="p:value"/>
    /// </xs:schema>"#;
    /// let set = SchemaSetBuilder::new()
    ///     .add_source(xsd, "file:///q.xsd")?
    ///     .compile()?
    ///     .into_schema_set();
    /// let names = &set.name_table;
    /// let q = set.lookup_element(Some(names.add("urn:t")), names.add("q")).unwrap();
    ///
    /// let vc = set.element_value_constraint(q).unwrap();
    /// assert_eq!(vc.variety, ValueConstraintVariety::Default);
    /// assert_eq!(vc.lexical_form, "p:value");
    /// assert!(vc.valid_default);
    /// // The QName `p:value` with `p` bound to `urn:p` where the schema wrote it:
    /// // a host that writes the element declares a prefix for `urn:p` on it.
    /// let Some(XmlValueKind::Atomic(XmlAtomicValue::QName(qn))) =
    ///     vc.typed_value.as_ref().map(|v| &v.value)
    /// else {
    ///     panic!("a QName value");
    /// };
    /// assert_eq!(qn.name().namespace_uri.map(|ns| names.resolve(ns)).as_deref(), Some("urn:p"));
    /// assert_eq!(qn.local_name(), "value");
    /// # Ok::<(), xsd_schema::SchemaError>(())
    /// ```
    pub fn element_value_constraint(&self, element: ElementKey) -> Option<ElementValueConstraint> {
        let decl = self.element_declaration(element)?;
        let record = self.value_constraint_record(decl)?;
        let declared = self.declared_type(decl);
        let valid_default = match self.content_of(declared) {
            Content::Simple => record.value.is_some(),
            Content::Mixed(ck) => self.emptiable(ck),
            Content::Other => false,
        };
        Some(ElementValueConstraint {
            variety: record.variety,
            lexical_form: record.lexical_form,
            typed_value: record.value,
            valid_default,
        })
    }

    /// The {value constraint} of the element declaration `element` as it
    /// applies to an element whose ·governing type definition· is
    /// `governing_type`: what validation gives such an element when it ends
    /// with neither element nor character children (and is not ·nilled·).
    ///
    /// The governing type is the declared one, or one that overrides it: an
    /// `xsi:type` (§3.3.4.1), an XSD 1.1 type alternative (§3.12.4) — the
    /// [`SchemaInfo::schema_type`](crate::validation::SchemaInfo::schema_type)
    /// of `validate_element`, replaced by that of
    /// `validate_end_of_attributes` when a type alternative selected one (see
    /// the [module documentation](self)).
    ///
    /// - {variety} and {lexical form}: the declaration's, as in
    ///   [`element_value_constraint`](Self::element_value_constraint).
    /// - `typed_value` (clause 5.1.2: "The element information item with D.{value
    ///   constraint}.{lexical form} used as its ·normalized value· is locally
    ///   ·valid· with respect to the ·governing type definition·"): the
    ///   {lexical form} read under `governing_type` — a QName / NOTATION with
    ///   the bindings of the schema element the value is written on — when
    ///   `governing_type` is simple or has simple content; that is the
    ///   element's
    ///   [`SchemaInfo::typed_value`](crate::validation::SchemaInfo::typed_value).
    ///   `None` when the form is not valid for it (the element is then
    ///   invalid), or when its content is not simple (no \[schema actual
    ///   value\]).
    /// - `valid_default` (clause 5.1.1): whether the value constraint is a
    ///   valid default for `governing_type` (Element Default Valid
    ///   (Immediate), §3.3.6.2) — required when an `xsi:type` gave the type
    ///   (an empty element is `cvc-elt.5.1.1` otherwise), not for a type
    ///   alternative's type.
    ///
    /// With the declared {type definition} it is
    /// [`element_value_constraint`](Self::element_value_constraint), except
    /// for a mixed content type: there the record's {value} is an `xs:string`
    /// and the element's `typed_value` is `None`.
    pub fn element_value_constraint_for_type(
        &self,
        element: ElementKey,
        governing_type: TypeKey,
    ) -> Option<ElementValueConstraint> {
        let decl = self.element_declaration(element)?;
        let record = self.value_constraint_record(decl)?;
        let (typed_value, valid_default) = match self.content_of(governing_type) {
            Content::Simple => {
                let value = self.read_value_constraint(decl, &record.lexical_form, governing_type);
                let maps_to_value = match (&value, &record.value) {
                    (Some(value), Some(declared)) => {
                        declared.type_code == XmlTypeCode::UntypedAtomic
                            || crate::validation::simple::typed_values_equal(declared, value)
                    }
                    _ => false,
                };
                (value, maps_to_value)
            }
            Content::Mixed(ck) => (None, self.emptiable(ck)),
            Content::Other => (None, false),
        };
        Some(ElementValueConstraint {
            variety: record.variety,
            lexical_form: record.lexical_form,
            typed_value,
            valid_default,
        })
    }

    /// D.{value constraint}.{lexical form} of `decl`, `None` without a value
    /// constraint — the ·normalized value· validation uses when the element
    /// takes the value constraint under another type than the declared one.
    pub(crate) fn value_constraint_lexical_form(&self, decl: &ElementDeclData) -> Option<String> {
        self.value_constraint_record(decl).map(|r| r.lexical_form)
    }

    /// D.{value constraint}.{lexical form} and D.{value constraint}.{value} of
    /// `decl` — the value a fixed value constraint compares an element's
    /// ·actual value· with (§3.3.4.3 clause 5.2.2.2.2) — `None` without a
    /// value constraint or when its {value} is not known.
    pub(crate) fn value_constraint_value(
        &self,
        decl: &ElementDeclData,
    ) -> Option<(String, XmlValue)> {
        self.value_constraint_record(decl)
            .and_then(|r| r.value.map(|value| (r.lexical_form, value)))
    }

    /// The declaration `element` names: itself, or the one a reference entry
    /// resolves to.
    fn element_declaration(&self, element: ElementKey) -> Option<&ElementDeclData> {
        let decl = self.arenas.elements.get(element)?;
        match decl.resolved_ref {
            Some(target) => self.arenas.elements.get(target),
            None => Some(decl),
        }
    }

    /// The declaration's {type definition}: `xs:anyType` when it names none.
    fn declared_type(&self, decl: &ElementDeclData) -> TypeKey {
        decl.resolved_type
            .unwrap_or_else(|| TypeKey::Complex(self.any_type_key()))
    }

    /// The {content type} variety of `type_key`, as Element Default Valid
    /// (Immediate) distinguishes it.
    fn content_of(&self, type_key: TypeKey) -> Content {
        let TypeKey::Complex(ck) = type_key else {
            return Content::Simple;
        };
        match self
            .arenas
            .complex_types
            .get(ck)
            .map(|ct| crate::validation::runtime::determine_content_type(self, ct))
        {
            Some(ContentType::TextOnly) => Content::Simple,
            Some(ContentType::Mixed) => Content::Mixed(ck),
            _ => Content::Other,
        }
    }

    /// Whether the particle of the complex type `ck` is ·emptiable· (§3.9.6.3)
    /// — whether its content model accepts no element at all.
    fn emptiable(&self, ck: crate::ids::ComplexTypeKey) -> bool {
        let Some(ct) = self.arenas.complex_types.get(ck) else {
            return false;
        };
        crate::compiler::compile_content_model_matcher(self, ct)
            .ok()
            .and_then(|matcher| {
                crate::validation::content::CompiledContentModel::try_from_matcher(matcher).ok()
            })
            .is_some_and(|model| {
                crate::validation::content::ContentValidatorState::from_compiled(&model)
                    .is_complete()
            })
    }

    /// The ·effective simple type definition· of `type_key` (§3.3.2.1): the
    /// type when it is simple, a complex type with simple content as itself
    /// (simple-type validation reads its {content type}'s simple type
    /// definition and the facets of its restrictions), else `xs:string`.
    fn effective_simple_type_definition(&self, type_key: TypeKey) -> TypeKey {
        match self.content_of(type_key) {
            Content::Simple => type_key,
            _ => TypeKey::Simple(self.builtin_types().string),
        }
    }

    /// The Value Constraint record of `decl` (§3.3.2.1), `None` without one.
    /// The literal is the attribute as written — `default` first, as
    /// validation takes it.
    fn value_constraint_record(&self, decl: &ElementDeclData) -> Option<Record> {
        let (variety, written) = match (&decl.default_value, &decl.fixed_value) {
            (Some(value), _) => (ValueConstraintVariety::Default, value.as_str()),
            (None, Some(value)) => (ValueConstraintVariety::Fixed, value.as_str()),
            (None, None) => return None,
        };
        let effective = self.effective_simple_type_definition(self.declared_type(decl));
        let (lexical_form, value) = match self.read_literal(decl, written, effective) {
            Some(result) => (
                result
                    .normalized_value
                    .clone()
                    .unwrap_or_else(|| written.to_owned()),
                value_of(result),
            ),
            None => (written.to_owned(), None),
        };
        Some(Record {
            variety,
            lexical_form,
            value,
        })
    }

    /// The {lexical form} `lexical` of `decl`'s value constraint read under the
    /// simple type or simple content `type_key`, with the declaration's
    /// bindings: its value, `None` when it is not valid there.
    fn read_value_constraint(
        &self,
        decl: &ElementDeclData,
        lexical: &str,
        type_key: TypeKey,
    ) -> Option<XmlValue> {
        self.read_literal(decl, lexical, type_key)
            .and_then(value_of)
    }

    /// `literal` validated against `type_key` with the namespace bindings of
    /// the schema element `decl` is written on.
    fn read_literal(
        &self,
        decl: &ElementDeclData,
        literal: &str,
        type_key: TypeKey,
    ) -> Option<crate::validation::simple::SimpleTypeResult> {
        self.read_literal_at(decl.source.as_ref(), literal, type_key)
    }

    /// Whether the `default` / `fixed` value `literal`, written on the schema
    /// element at `source` (an element declaration, an attribute declaration
    /// or an attribute use), has a value in `type_key` — Simple Default Valid
    /// (§3.2.6.2): "1 V.{lexical form} is ·valid· with respect to T … 2
    /// V.{lexical form} maps to V.{value} in the value space of T". A QName /
    /// NOTATION in it maps to a value only with the bindings of that schema
    /// element (Datatypes §3.3.18), so a prefix bound nowhere there leaves it
    /// without one. Without those bindings (a declaration not read from a
    /// schema document) a QName is taken as written.
    pub(crate) fn value_constraint_literal_has_value(
        &self,
        literal: &str,
        type_key: TypeKey,
        source: Option<&crate::parser::location::SourceRef>,
    ) -> bool {
        let Some(result) = self.read_literal_at(source, literal, type_key) else {
            return false;
        };
        self.value_constraint_namespaces(source).is_none()
            || !crate::validation::simple::holds_lexical_qname(&result.typed_value)
    }

    /// `literal` validated against `type_key` with the namespace bindings of
    /// the schema element at `source`.
    fn read_literal_at(
        &self,
        source: Option<&crate::parser::location::SourceRef>,
        literal: &str,
        type_key: TypeKey,
    ) -> Option<crate::validation::simple::SimpleTypeResult> {
        let bindings = self.value_constraint_namespaces(source);
        let resolver =
            bindings.map(|ns| crate::validation::simple::snapshot_prefixes(ns, &self.name_table));
        crate::validation::simple::validate_simple_type_in(
            literal,
            type_key,
            self,
            resolver
                .as_ref()
                .map(|r| r as crate::validation::simple::PrefixResolver<'_>),
        )
        .ok()
    }
}

/// A value constraint's variety, lexical form and {value}.
struct Record {
    variety: ValueConstraintVariety,
    lexical_form: String,
    value: Option<XmlValue>,
}

/// A type's {content type} variety, as Element Default Valid (Immediate)
/// (§3.3.6.2) distinguishes it.
enum Content {
    /// A simple type, or a complex type with simple content.
    Simple,
    /// Mixed content (of this complex type).
    Mixed(crate::ids::ComplexTypeKey),
    /// Empty or element-only content.
    Other,
}

/// The value of a validation result, unless it still holds a QName /
/// NOTATION in lexical form — one whose prefix is bound nowhere at the
/// declaration (or the declaration's bindings are not known).
fn value_of(result: crate::validation::simple::SimpleTypeResult) -> Option<XmlValue> {
    Some(result.typed_value).filter(|value| !crate::validation::simple::holds_lexical_qname(value))
}
