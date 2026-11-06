//! Built-in type registry for XSD types
//!
//! This module provides the `BuiltinTypes` struct which contains pre-allocated
//! `SimpleTypeKey` references for all 50 built-in XSD simple types, plus
//! the built-in complex `xs:anyType`.
//!
//! Every set also has the types the XPath 2.0 data model adds to the XML
//! Schema namespace (XDM 1.0 §2.6.2) — `xs:anyAtomicType`,
//! `xs:untypedAtomic`, `xs:dayTimeDuration`, `xs:yearMonthDuration` — as
//! components, for a host that validates nodes of the data model against a
//! type it names ([`BuiltinTypes::xpath_type`]). A schema document reaches
//! only the ones its XSD version defines: none of them in XSD 1.0, the first,
//! third and fourth in XSD 1.1 (Datatypes 1.1 §3.2.2, §3.4.26, §3.4.27) —
//! never `xs:untypedAtomic`, which no XSD version defines.
//!
//! ## Usage
//!
//! Built-in types are initialized when a `SchemaSet` is created and can be
//! accessed via `schema_set.builtin_types()`.
//!
//! ```rust
//! use xsd_schema::SchemaSet;
//!
//! let schema_set = SchemaSet::new();
//! let string_key = schema_set.builtin_types().string;
//! ```

use std::collections::HashMap;

use ahash::RandomState;
use slotmap::SecondaryMap;

use super::{BuiltInType, XmlTypeCode};
use crate::arenas::{ComplexTypeDefData, SimpleTypeDefData};
use crate::ids::{AttributeKey, ComplexTypeKey, NameId, SimpleTypeKey, TypeKey};
use crate::namespace::table::well_known;
use crate::parser::frames::{
    ComplexContentDefResult, ComplexContentResult, DerivationMethod, ParticleResult, ParticleTerm,
    ProcessContents, WildcardNamespace, WildcardResult,
};
use crate::schema::model::{SchemaSet, XsdVersion};

/// Well-known built-in type IDs for fast access.
///
/// This struct contains `SimpleTypeKey` references for all 50 built-in XSD
/// simple types. It is initialized when a `SchemaSet` is created.
///
/// Types are organized by category:
/// - Abstract types (anySimpleType, anyAtomicType)
/// - String types (string, normalizedString, token, etc.)
/// - Numeric types (boolean, decimal, float, double, integer hierarchy)
/// - Date/time types (duration, dateTime, date, time, gregorian types)
/// - Binary types (hexBinary, base64Binary)
/// - Other types (anyURI, QName, NOTATION)
/// - List types (NMTOKENS, IDREFS, ENTITIES)
#[derive(Debug, Clone)]
pub struct BuiltinTypes {
    // Complex types
    /// xs:anyType - the ur-type
    pub any_type: ComplexTypeKey,

    // Abstract types
    /// xs:anySimpleType - base of all simple types
    pub any_simple_type: SimpleTypeKey,
    /// xs:anyAtomicType - base of all atomic types (XSD 1.1). `None` in an
    /// XSD 1.0 set, whose schema documents cannot name it; a host finds the
    /// set's component with [`xpath_type`](Self::xpath_type).
    pub any_atomic_type: Option<SimpleTypeKey>,

    // String types
    /// xs:string
    pub string: SimpleTypeKey,
    /// xs:normalizedString
    pub normalized_string: SimpleTypeKey,
    /// xs:token
    pub token: SimpleTypeKey,
    /// xs:language
    pub language: SimpleTypeKey,
    /// xs:NMTOKEN
    pub nmtoken: SimpleTypeKey,
    /// xs:Name
    pub name: SimpleTypeKey,
    /// xs:NCName
    pub ncname: SimpleTypeKey,
    /// xs:ID
    pub id: SimpleTypeKey,
    /// xs:IDREF
    pub idref: SimpleTypeKey,
    /// xs:ENTITY
    pub entity: SimpleTypeKey,

    // Numeric types
    /// xs:boolean
    pub boolean: SimpleTypeKey,
    /// xs:decimal
    pub decimal: SimpleTypeKey,
    /// xs:float
    pub float: SimpleTypeKey,
    /// xs:double
    pub double: SimpleTypeKey,
    /// xs:integer
    pub integer: SimpleTypeKey,
    /// xs:nonPositiveInteger
    pub non_positive_integer: SimpleTypeKey,
    /// xs:negativeInteger
    pub negative_integer: SimpleTypeKey,
    /// xs:long
    pub long: SimpleTypeKey,
    /// xs:int
    pub int: SimpleTypeKey,
    /// xs:short
    pub short: SimpleTypeKey,
    /// xs:byte
    pub byte: SimpleTypeKey,
    /// xs:nonNegativeInteger
    pub non_negative_integer: SimpleTypeKey,
    /// xs:unsignedLong
    pub unsigned_long: SimpleTypeKey,
    /// xs:unsignedInt
    pub unsigned_int: SimpleTypeKey,
    /// xs:unsignedShort
    pub unsigned_short: SimpleTypeKey,
    /// xs:unsignedByte
    pub unsigned_byte: SimpleTypeKey,
    /// xs:positiveInteger
    pub positive_integer: SimpleTypeKey,

    // Date/time types
    /// xs:duration
    pub duration: SimpleTypeKey,
    /// xs:dateTime
    pub datetime: SimpleTypeKey,
    /// xs:time
    pub time: SimpleTypeKey,
    /// xs:date
    pub date: SimpleTypeKey,
    /// xs:gYearMonth
    pub g_year_month: SimpleTypeKey,
    /// xs:gYear
    pub g_year: SimpleTypeKey,
    /// xs:gMonthDay
    pub g_month_day: SimpleTypeKey,
    /// xs:gDay
    pub g_day: SimpleTypeKey,
    /// xs:gMonth
    pub g_month: SimpleTypeKey,

    // XSD 1.1 date/time additions
    /// xs:yearMonthDuration (XSD 1.1). `None` in an XSD 1.0 set; a host finds
    /// the set's component with [`xpath_type`](Self::xpath_type).
    pub year_month_duration: Option<SimpleTypeKey>,
    /// xs:dayTimeDuration (XSD 1.1). `None` in an XSD 1.0 set; a host finds
    /// the set's component with [`xpath_type`](Self::xpath_type).
    pub day_time_duration: Option<SimpleTypeKey>,
    /// xs:dateTimeStamp (XSD 1.1)
    pub datetime_stamp: Option<SimpleTypeKey>,
    /// xs:untypedAtomic, the XPath 2.0 data model's type (XDM 1.0 §2.6.2),
    /// which no XML Schema version defines: schema documents cannot name it
    /// (`src-resolve`) in either version. `Some` in an XSD 1.1 set and `None`
    /// in an XSD 1.0 set, as before; a host finds the set's component in both
    /// with [`xpath_type`](Self::xpath_type).
    pub untyped_atomic: Option<SimpleTypeKey>,

    // XSD 1.1 bottom type
    /// xs:error - the bottom type (union of no members); has no valid values (XSD 1.1)
    pub error: Option<SimpleTypeKey>,

    // Binary types
    /// xs:hexBinary
    pub hex_binary: SimpleTypeKey,
    /// xs:base64Binary
    pub base64_binary: SimpleTypeKey,

    // URI types
    /// xs:anyURI
    pub any_uri: SimpleTypeKey,

    /// Anonymous `list(xs:anyURI)` — type of `xsi:schemaLocation` built-in attribute
    pub xsi_schema_location_type: SimpleTypeKey,

    // QName types
    /// xs:QName
    pub qname: SimpleTypeKey,
    /// xs:NOTATION
    pub notation: SimpleTypeKey,

    // List types (built-in)
    /// xs:NMTOKENS
    pub nmtokens: SimpleTypeKey,
    /// xs:IDREFS
    pub idrefs: SimpleTypeKey,
    /// xs:ENTITIES
    pub entities: SimpleTypeKey,

    // Lookup maps for fast access. Keys are small internal integers (enum
    // discriminants / dense indices), so these use `ahash` (fast, keyed) instead
    // of SipHash, and a dense `SecondaryMap` where the key is a slotmap key.
    /// Map from XmlTypeCode to SimpleTypeKey
    by_type_code: HashMap<XmlTypeCode, SimpleTypeKey, RandomState>,
    /// Reverse map from SimpleTypeKey to XmlTypeCode, so `get_type_code` is O(1).
    /// Dense array index by slotmap key — zero hashing.
    by_key: SecondaryMap<SimpleTypeKey, XmlTypeCode>,
    /// Map from local name NameId to SimpleTypeKey (for XS namespace)
    by_local_name: HashMap<NameId, SimpleTypeKey, RandomState>,
    /// The XPath 2.0 data model's types (XDM 1.0 §2.6.2), present in every
    /// set: in an XSD 1.1 set the same keys as the public fields; in an XSD
    /// 1.0 set components that only a host reaches ([`Self::xpath_type`]).
    xpath: XPathTypeKeys,

    // Built-in XSI attribute declarations (§3.2.7)
    /// `xsi:type` built-in attribute declaration
    pub xsi_type_attr: AttributeKey,
    /// `xsi:nil` built-in attribute declaration
    pub xsi_nil_attr: AttributeKey,
    /// `xsi:schemaLocation` built-in attribute declaration
    pub xsi_schema_location_attr: AttributeKey,
    /// `xsi:noNamespaceSchemaLocation` built-in attribute declaration
    pub xsi_no_namespace_schema_location_attr: AttributeKey,
}

/// The keys of the four types XPath 2.0's data model adds to the XML Schema
/// namespace (`xs:untyped` is no simple type: a host's untyped element).
#[derive(Debug, Clone, Copy)]
struct XPathTypeKeys {
    any_atomic_type: SimpleTypeKey,
    untyped_atomic: SimpleTypeKey,
    day_time_duration: SimpleTypeKey,
    year_month_duration: SimpleTypeKey,
}

impl XPathTypeKeys {
    fn get(&self, code: XmlTypeCode) -> Option<SimpleTypeKey> {
        match code {
            XmlTypeCode::AnyAtomicType => Some(self.any_atomic_type),
            XmlTypeCode::UntypedAtomic => Some(self.untyped_atomic),
            XmlTypeCode::DayTimeDuration => Some(self.day_time_duration),
            XmlTypeCode::YearMonthDuration => Some(self.year_month_duration),
            _ => None,
        }
    }
}

impl BuiltinTypes {
    /// Initialize all built-in types and register them in the schema set.
    ///
    /// This creates `SimpleTypeDefData` entries in the arenas for the 45
    /// XSD 1.0 built-in simple types and the four types of the XPath 2.0 data
    /// model ([`xpath_type`](Self::xpath_type)), plus `xs:dateTimeStamp` and
    /// `xs:error` if the version is XSD 1.1.
    pub fn new(schema_set: &mut SchemaSet) -> Self {
        let xsd_version = schema_set.xsd_version;
        let xs_ns = Some(well_known::XS_NAMESPACE);

        let any_type_name = schema_set.name_table.add("anyType");
        let any_type = schema_set.arenas.alloc_complex_type(ComplexTypeDefData {
            name: Some(any_type_name),
            target_namespace: xs_ns,
            base_type: None,
            derivation_method: None,
            content: ComplexContentResult::Complex(ComplexContentDefResult {
                particle: Some(ParticleResult {
                    term: ParticleTerm::Any(WildcardResult {
                        namespace: WildcardNamespace::Any,
                        process_contents: ProcessContents::Lax,
                        not_namespace: Vec::new(),
                        not_qname: Vec::new(),
                        id: None,
                        annotation: None,
                        source: None,
                    }),
                    min_occurs: 0,
                    max_occurs: None,
                    source: None,
                }),
                derivation: DerivationMethod::Restriction,
                mixed: true,
                mixed_explicit: true,
                base_type: None,
                open_content: None,
                attributes: Vec::new(),
                attribute_groups: Vec::new(),
                attribute_wildcard: Some(WildcardResult {
                    namespace: WildcardNamespace::Any,
                    process_contents: ProcessContents::Lax,
                    not_namespace: Vec::new(),
                    not_qname: Vec::new(),
                    id: None,
                    annotation: None,
                    source: None,
                }),
                assertions: Vec::new(),
                id: None,
                derivation_id: None,
                source: None,
            }),
            open_content: None,
            attributes: Vec::new(),
            attribute_groups: Vec::new(),
            attribute_wildcard: None,
            mixed: true,
            is_abstract: false,
            final_derivation: crate::schema::model::DerivationSet::empty(),
            block: crate::schema::model::DerivationSet::empty(),
            default_attributes_apply: true,
            id: None,
            #[cfg(feature = "xsd11")]
            assertions: Vec::new(),
            #[cfg(feature = "xsd11")]
            xpath_default_namespace: None,
            annotation: None,
            source: None,
            // Resolved references (built-in types have no unresolved references)
            resolved_base_type: None,
            resolved_attribute_groups: Vec::new(),
            resolved_attributes: Vec::new(),
            resolved_content_particle_types: Vec::new(),
            resolved_content_particle_elements: Vec::new(),
            resolved_simple_content_type: None,
            redefine_original: None,
        });

        // Helper to create and register a built-in type
        let mut create_type = |builtin: BuiltInType| -> SimpleTypeKey {
            let local_name = builtin.local_name();
            let name_id = schema_set.name_table.add(local_name);
            let variety = match builtin {
                BuiltInType::NMTOKENS | BuiltInType::IDREFS | BuiltInType::ENTITIES => {
                    crate::parser::frames::SimpleTypeVariety::List
                }
                BuiltInType::XsError => crate::parser::frames::SimpleTypeVariety::Union,
                _ => crate::parser::frames::SimpleTypeVariety::Atomic,
            };

            let data = SimpleTypeDefData {
                name: Some(name_id),
                target_namespace: xs_ns,
                variety,
                base_type: None,
                item_type: None,
                member_types: Vec::new(),
                facets: crate::types::simple::effective_arena_facets_for_builtin(builtin),
                final_derivation: crate::schema::model::DerivationSet::empty(),
                id: None,
                derivation_id: None,
                annotation: None,
                source: None,
                // Resolved references (built-in types have no unresolved references)
                resolved_base_type: None,
                resolved_item_type: None,
                resolved_member_types: Vec::new(),
                redefine_original: None,
                deferred_item_type_error: None,
            };

            schema_set.arenas.alloc_simple_type(data)
        };

        // Create all XSD 1.0 types
        let any_simple_type = create_type(BuiltInType::AnySimpleType);
        let string = create_type(BuiltInType::String);
        let normalized_string = create_type(BuiltInType::NormalizedString);
        let token = create_type(BuiltInType::Token);
        let language = create_type(BuiltInType::Language);
        let nmtoken = create_type(BuiltInType::NMTOKEN);
        let name = create_type(BuiltInType::Name);
        let ncname = create_type(BuiltInType::NCName);
        let id = create_type(BuiltInType::ID);
        let idref = create_type(BuiltInType::IDREF);
        let entity = create_type(BuiltInType::ENTITY);

        let boolean = create_type(BuiltInType::Boolean);
        let decimal = create_type(BuiltInType::Decimal);
        let float = create_type(BuiltInType::Float);
        let double = create_type(BuiltInType::Double);
        let integer = create_type(BuiltInType::Integer);
        let non_positive_integer = create_type(BuiltInType::NonPositiveInteger);
        let negative_integer = create_type(BuiltInType::NegativeInteger);
        let long = create_type(BuiltInType::Long);
        let int = create_type(BuiltInType::Int);
        let short = create_type(BuiltInType::Short);
        let byte = create_type(BuiltInType::Byte);
        let non_negative_integer = create_type(BuiltInType::NonNegativeInteger);
        let unsigned_long = create_type(BuiltInType::UnsignedLong);
        let unsigned_int = create_type(BuiltInType::UnsignedInt);
        let unsigned_short = create_type(BuiltInType::UnsignedShort);
        let unsigned_byte = create_type(BuiltInType::UnsignedByte);
        let positive_integer = create_type(BuiltInType::PositiveInteger);

        let duration = create_type(BuiltInType::Duration);
        let datetime = create_type(BuiltInType::DateTime);
        let time = create_type(BuiltInType::Time);
        let date = create_type(BuiltInType::Date);
        let g_year_month = create_type(BuiltInType::GYearMonth);
        let g_year = create_type(BuiltInType::GYear);
        let g_month_day = create_type(BuiltInType::GMonthDay);
        let g_day = create_type(BuiltInType::GDay);
        let g_month = create_type(BuiltInType::GMonth);

        let hex_binary = create_type(BuiltInType::HexBinary);
        let base64_binary = create_type(BuiltInType::Base64Binary);

        let any_uri = create_type(BuiltInType::AnyURI);
        let qname = create_type(BuiltInType::QName);
        let notation = create_type(BuiltInType::NOTATION);

        let nmtokens = create_type(BuiltInType::NMTOKENS);
        let idrefs = create_type(BuiltInType::IDREFS);
        let entities = create_type(BuiltInType::ENTITIES);

        // The XPath 2.0 data model's types (XDM 1.0 §2.6.2), in every set.
        let xpath = XPathTypeKeys {
            any_atomic_type: create_type(BuiltInType::AnyAtomicType),
            untyped_atomic: create_type(BuiltInType::UntypedAtomic),
            year_month_duration: create_type(BuiltInType::YearMonthDuration),
            day_time_duration: create_type(BuiltInType::DayTimeDuration),
        };

        // The XSD 1.1 built-ins: three of the data model's types (Datatypes
        // 1.1 §3.2.2, §3.4.26, §3.4.27), `xs:dateTimeStamp`, `xs:error`. The
        // field `untyped_atomic` is filled in 1.1 too, but XSD 1.1 does not
        // define that type (below: no entry by name).
        let v11 = xsd_version == XsdVersion::V1_1;
        let (
            any_atomic_type,
            untyped_atomic,
            year_month_duration,
            day_time_duration,
            datetime_stamp,
            error,
        ) = if v11 {
            (
                Some(xpath.any_atomic_type),
                Some(xpath.untyped_atomic),
                Some(xpath.year_month_duration),
                Some(xpath.day_time_duration),
                Some(create_type(BuiltInType::DateTimeStamp)),
                Some(create_type(BuiltInType::XsError)),
            )
        } else {
            (None, None, None, None, None, None)
        };

        // Anonymous list(anyURI) for xsi:schemaLocation built-in attribute type.
        // Created after `create_type` closure is no longer needed.
        let xsi_schema_location_type = {
            let data = SimpleTypeDefData {
                name: None,
                target_namespace: None,
                variety: crate::parser::frames::SimpleTypeVariety::List,
                base_type: None,
                item_type: None,
                member_types: Vec::new(),
                facets: crate::types::facets::FacetSet::new(),
                final_derivation: crate::schema::model::DerivationSet::empty(),
                id: None,
                derivation_id: None,
                annotation: None,
                source: None,
                resolved_base_type: None,
                resolved_item_type: None,
                resolved_member_types: Vec::new(),
                redefine_original: None,
                deferred_item_type_error: None,
            };
            schema_set.arenas.alloc_simple_type(data)
        };

        // Build lookup maps (ahash for the small-int keyed maps; dense
        // SecondaryMap for the slotmap-keyed reverse map — no SipHash).
        let mut by_type_code: HashMap<XmlTypeCode, SimpleTypeKey, RandomState> = HashMap::default();
        let mut by_key: SecondaryMap<SimpleTypeKey, XmlTypeCode> = SecondaryMap::new();
        let mut by_local_name: HashMap<NameId, SimpleTypeKey, RandomState> = HashMap::default();

        // Helper to add to lookup maps
        let mut add_to_maps = |builtin: BuiltInType, key: SimpleTypeKey| {
            let type_code = builtin.type_code();
            by_type_code.insert(type_code, key);
            by_key.insert(key, type_code);

            let local_name = builtin.local_name();
            let name_id = schema_set.name_table.add(local_name);
            by_local_name.insert(name_id, key);
        };

        // Add all types to maps
        add_to_maps(BuiltInType::AnySimpleType, any_simple_type);
        add_to_maps(BuiltInType::String, string);
        add_to_maps(BuiltInType::NormalizedString, normalized_string);
        add_to_maps(BuiltInType::Token, token);
        add_to_maps(BuiltInType::Language, language);
        add_to_maps(BuiltInType::NMTOKEN, nmtoken);
        add_to_maps(BuiltInType::Name, name);
        add_to_maps(BuiltInType::NCName, ncname);
        add_to_maps(BuiltInType::ID, id);
        add_to_maps(BuiltInType::IDREF, idref);
        add_to_maps(BuiltInType::ENTITY, entity);
        add_to_maps(BuiltInType::Boolean, boolean);
        add_to_maps(BuiltInType::Decimal, decimal);
        add_to_maps(BuiltInType::Float, float);
        add_to_maps(BuiltInType::Double, double);
        add_to_maps(BuiltInType::Integer, integer);
        add_to_maps(BuiltInType::NonPositiveInteger, non_positive_integer);
        add_to_maps(BuiltInType::NegativeInteger, negative_integer);
        add_to_maps(BuiltInType::Long, long);
        add_to_maps(BuiltInType::Int, int);
        add_to_maps(BuiltInType::Short, short);
        add_to_maps(BuiltInType::Byte, byte);
        add_to_maps(BuiltInType::NonNegativeInteger, non_negative_integer);
        add_to_maps(BuiltInType::UnsignedLong, unsigned_long);
        add_to_maps(BuiltInType::UnsignedInt, unsigned_int);
        add_to_maps(BuiltInType::UnsignedShort, unsigned_short);
        add_to_maps(BuiltInType::UnsignedByte, unsigned_byte);
        add_to_maps(BuiltInType::PositiveInteger, positive_integer);
        add_to_maps(BuiltInType::Duration, duration);
        add_to_maps(BuiltInType::DateTime, datetime);
        add_to_maps(BuiltInType::Time, time);
        add_to_maps(BuiltInType::Date, date);
        add_to_maps(BuiltInType::GYearMonth, g_year_month);
        add_to_maps(BuiltInType::GYear, g_year);
        add_to_maps(BuiltInType::GMonthDay, g_month_day);
        add_to_maps(BuiltInType::GDay, g_day);
        add_to_maps(BuiltInType::GMonth, g_month);
        add_to_maps(BuiltInType::HexBinary, hex_binary);
        add_to_maps(BuiltInType::Base64Binary, base64_binary);
        add_to_maps(BuiltInType::AnyURI, any_uri);
        add_to_maps(BuiltInType::QName, qname);
        add_to_maps(BuiltInType::NOTATION, notation);
        add_to_maps(BuiltInType::NMTOKENS, nmtokens);
        add_to_maps(BuiltInType::IDREFS, idrefs);
        add_to_maps(BuiltInType::ENTITIES, entities);

        // Add XSD 1.1 types to maps
        if let Some(key) = any_atomic_type {
            add_to_maps(BuiltInType::AnyAtomicType, key);
        }
        if let Some(key) = year_month_duration {
            add_to_maps(BuiltInType::YearMonthDuration, key);
        }
        if let Some(key) = day_time_duration {
            add_to_maps(BuiltInType::DayTimeDuration, key);
        }
        if let Some(key) = datetime_stamp {
            add_to_maps(BuiltInType::DateTimeStamp, key);
        }
        if let Some(key) = error {
            add_to_maps(BuiltInType::XsError, key);
        }
        // `xs:untypedAtomic` has no entry by name in either version, so that a
        // schema document's QName naming it does not resolve — Structures 1.1
        // §1.3.1.1: the data model "makes use of type labels in the XSD
        // namespace (untyped, untypedAtomic) which are not defined in this
        // specification"; §3.17.6.2 QName resolution (Schema Document), clause
        // 1.1: a QName resolves to a type definition only if "That component
        // is a member of the value of the appropriate property of the schema
        // … {type definitions}", and the type definitions present in every
        // schema by definition (§3.16.7) do not include it — `src-resolve`.
        // An XSD 1.1 set keeps it by code (its field and
        // `get_by_type_code`, as before); a host reaches it in both versions
        // through `xpath_type`.
        if let Some(key) = untyped_atomic {
            by_type_code.insert(XmlTypeCode::UntypedAtomic, key);
            by_key.insert(key, XmlTypeCode::UntypedAtomic);
        }
        // In an XSD 1.0 set the data model's types have a type code (so an
        // annotation names its datatype) but no entry by code or by name: a
        // schema document, an `xsi:type` and the code lookups of schema
        // processing do not find them.
        if !v11 {
            by_key.insert(xpath.any_atomic_type, XmlTypeCode::AnyAtomicType);
            by_key.insert(xpath.untyped_atomic, XmlTypeCode::UntypedAtomic);
            by_key.insert(xpath.year_month_duration, XmlTypeCode::YearMonthDuration);
            by_key.insert(xpath.day_time_duration, XmlTypeCode::DayTimeDuration);
        }

        // Resolve item types for built-in list types so that
        // validate_list_type can validate each item individually.
        for (list_key, item_key) in [
            (nmtokens, nmtoken),
            (idrefs, idref),
            (entities, entity),
            (xsi_schema_location_type, any_uri),
        ] {
            if let Some(st) = schema_set
                .arenas
                .entries_mut()
                .get_simple_type_mut(list_key)
            {
                st.resolved_item_type = Some(TypeKey::Simple(item_key));
            }
        }

        // Allocate built-in XSI attribute declarations (§3.2.7)
        let xsi_ns = Some(well_known::XSI_NAMESPACE);
        let xsi_type_attr = schema_set
            .arenas
            .alloc_attribute(crate::arenas::AttributeDeclData {
                name: Some(well_known::XSI_TYPE),
                target_namespace: xsi_ns,
                ref_name: None,
                type_ref: None,
                inline_type: None,
                default_value: None,
                fixed_value: None,
                use_kind: None,
                form: None,
                inheritable: false,
                id: None,
                annotation: None,
                source: None,
                resolved_type: Some(TypeKey::Simple(qname)),
                resolved_ref: None,
            });
        let xsi_nil_attr = schema_set
            .arenas
            .alloc_attribute(crate::arenas::AttributeDeclData {
                name: Some(well_known::XSI_NIL),
                target_namespace: xsi_ns,
                ref_name: None,
                type_ref: None,
                inline_type: None,
                default_value: None,
                fixed_value: None,
                use_kind: None,
                form: None,
                inheritable: false,
                id: None,
                annotation: None,
                source: None,
                resolved_type: Some(TypeKey::Simple(boolean)),
                resolved_ref: None,
            });
        let xsi_schema_location_attr =
            schema_set
                .arenas
                .alloc_attribute(crate::arenas::AttributeDeclData {
                    name: Some(well_known::XSI_SCHEMA_LOCATION),
                    target_namespace: xsi_ns,
                    ref_name: None,
                    type_ref: None,
                    inline_type: None,
                    default_value: None,
                    fixed_value: None,
                    use_kind: None,
                    form: None,
                    inheritable: false,
                    id: None,
                    annotation: None,
                    source: None,
                    resolved_type: Some(TypeKey::Simple(xsi_schema_location_type)),
                    resolved_ref: None,
                });
        let xsi_no_namespace_schema_location_attr =
            schema_set
                .arenas
                .alloc_attribute(crate::arenas::AttributeDeclData {
                    name: Some(well_known::XSI_NO_NAMESPACE_SCHEMA_LOCATION),
                    target_namespace: xsi_ns,
                    ref_name: None,
                    type_ref: None,
                    inline_type: None,
                    default_value: None,
                    fixed_value: None,
                    use_kind: None,
                    form: None,
                    inheritable: false,
                    id: None,
                    annotation: None,
                    source: None,
                    resolved_type: Some(TypeKey::Simple(any_uri)),
                    resolved_ref: None,
                });

        // Register XSI attribute declarations in the XSI namespace table
        // so that lookup_attribute(XSI_NAMESPACE, name) finds them.
        {
            let xsi_ns_table = schema_set.get_or_create_namespace(Some(well_known::XSI_NAMESPACE));
            xsi_ns_table
                .attributes
                .insert(well_known::XSI_TYPE, xsi_type_attr);
            xsi_ns_table
                .attributes
                .insert(well_known::XSI_NIL, xsi_nil_attr);
            xsi_ns_table
                .attributes
                .insert(well_known::XSI_SCHEMA_LOCATION, xsi_schema_location_attr);
            xsi_ns_table.attributes.insert(
                well_known::XSI_NO_NAMESPACE_SCHEMA_LOCATION,
                xsi_no_namespace_schema_location_attr,
            );
        }

        Self {
            any_type,
            any_simple_type,
            any_atomic_type,
            string,
            normalized_string,
            token,
            language,
            nmtoken,
            name,
            ncname,
            id,
            idref,
            entity,
            boolean,
            decimal,
            float,
            double,
            integer,
            non_positive_integer,
            negative_integer,
            long,
            int,
            short,
            byte,
            non_negative_integer,
            unsigned_long,
            unsigned_int,
            unsigned_short,
            unsigned_byte,
            positive_integer,
            duration,
            datetime,
            time,
            date,
            g_year_month,
            g_year,
            g_month_day,
            g_day,
            g_month,
            year_month_duration,
            day_time_duration,
            datetime_stamp,
            untyped_atomic,
            error,
            hex_binary,
            base64_binary,
            any_uri,
            xsi_schema_location_type,
            qname,
            notation,
            nmtokens,
            idrefs,
            entities,
            by_type_code,
            by_key,
            by_local_name,
            xpath,
            xsi_type_attr,
            xsi_nil_attr,
            xsi_schema_location_attr,
            xsi_no_namespace_schema_location_attr,
        }
    }

    /// Get a built-in type by its XmlTypeCode.
    ///
    /// Returns `None` for node types, AnyType, and other non-simple type codes,
    /// and for a type the set's XSD version does not define: in an XSD 1.0 set
    /// the four types of the XPath 2.0 data model, which
    /// [`xpath_type`](Self::xpath_type) finds. (An XSD 1.1 set answers for
    /// `xs:untypedAtomic` too, which its schema documents cannot name.)
    pub fn get_by_type_code(&self, code: XmlTypeCode) -> Option<SimpleTypeKey> {
        self.by_type_code.get(&code).copied()
    }

    /// The simple type with this code as a host of the XPath 2.0 data model
    /// sees the set: every built-in simple type [`get_by_type_code`](Self::get_by_type_code)
    /// finds, and the four types the data model adds to the XML Schema
    /// namespace (XDM 1.0 §2.6.2: "the data model defines five additional
    /// types: xs:anyAtomicType, xs:untyped, xs:untypedAtomic,
    /// xs:dayTimeDuration, and xs:yearMonthDuration"; `xs:untyped` is no
    /// simple type) in both XSD versions.
    ///
    /// In an XSD 1.1 set those are the set's own components (the fields
    /// [`any_atomic_type`](Self::any_atomic_type) …), of which its schema
    /// documents name all but `xs:untypedAtomic`. In an XSD 1.0 set,
    /// whose schema documents cannot name them (XML Schema 1.0 has none of
    /// them), they are components only a host reaches: validation against
    /// them, annotations, typed values and the type hierarchy work as in a
    /// 1.1 set; their places in it are XDM's — `xs:dayTimeDuration` and
    /// `xs:yearMonthDuration` restrict `xs:duration`, `xs:untypedAtomic`
    /// restricts `xs:anyAtomicType`, and every atomic type derives from
    /// `xs:anyAtomicType`, which restricts `xs:anySimpleType`.
    pub fn xpath_type(&self, code: XmlTypeCode) -> Option<SimpleTypeKey> {
        self.get_by_type_code(code).or_else(|| self.xpath.get(code))
    }

    /// Whether `key` is one of the four types of the XPath 2.0 data model.
    pub(crate) fn is_xpath_type(&self, key: SimpleTypeKey) -> bool {
        key == self.xpath.any_atomic_type
            || key == self.xpath.untyped_atomic
            || key == self.xpath.day_time_duration
            || key == self.xpath.year_month_duration
    }

    /// In an XSD 1.0 set, whether `key` is the data model's
    /// `xs:anyAtomicType`, which the {base type definition} chains of XSD 1.0
    /// do not pass through (a 1.0 primitive's base is `xs:anySimpleType`):
    /// derivation from it is decided by variety instead
    /// ([`xpath_type`](Self::xpath_type)).
    pub(crate) fn is_detached_any_atomic_type(&self, key: SimpleTypeKey) -> bool {
        self.any_atomic_type.is_none() && key == self.xpath.any_atomic_type
    }

    /// Get a built-in type by its local name (within the XS namespace).
    ///
    /// The local name must be interned in the NameTable (passed as NameId).
    pub fn get_by_local_name(&self, name: NameId) -> Option<SimpleTypeKey> {
        self.by_local_name.get(&name).copied()
    }

    /// Get the XmlTypeCode for a built-in type key.
    ///
    /// Returns `None` if the key is not a built-in type.
    pub fn get_type_code(&self, key: SimpleTypeKey) -> Option<XmlTypeCode> {
        self.by_key.get(key).copied()
    }

    /// Check if a type key is a built-in type.
    pub fn is_builtin(&self, key: SimpleTypeKey) -> bool {
        self.get_type_code(key).is_some()
    }

    /// True when `key` is one of the four attribute declarations present in
    /// every schema by definition (Structures 1.1 §3.2.7): `xsi:type`,
    /// `xsi:nil`, `xsi:schemaLocation`, `xsi:noNamespaceSchemaLocation`.
    #[cfg(feature = "xsd11")]
    pub(crate) fn is_builtin_attribute(&self, key: AttributeKey) -> bool {
        [
            self.xsi_type_attr,
            self.xsi_nil_attr,
            self.xsi_schema_location_attr,
            self.xsi_no_namespace_schema_location_attr,
        ]
        .contains(&key)
    }

    /// True when `key` is the `xs:anyAtomicType` built-in (XSD 1.1 only).
    #[inline]
    pub fn is_any_atomic_type(&self, key: SimpleTypeKey) -> bool {
        self.any_atomic_type == Some(key)
    }

    /// Get the base type for a built-in type (for derivation hierarchy).
    ///
    /// Returns the immediate base type in the XSD type hierarchy.
    /// Returns `None` for `anySimpleType` (the root of simple types).
    ///
    /// In XSD 1.1 mode, primitive atomic types derive from `xs:anyAtomicType`
    /// per §3.16.7.3; in XSD 1.0 they derive from `xs:anySimpleType`.
    ///
    /// The data model's types of an XSD 1.0 set ([`xpath_type`](Self::xpath_type))
    /// have their XDM bases: `xs:untypedAtomic` → `xs:anyAtomicType` →
    /// `xs:anySimpleType`, the durations → `xs:duration`.
    pub fn get_base_type(&self, key: SimpleTypeKey) -> Option<SimpleTypeKey> {
        let code = self.get_type_code(key)?;
        let base_code = get_builtin_base_type(code)?;
        if base_code == XmlTypeCode::AnySimpleType
            && code.is_primitive_atomic()
            && self.any_atomic_type.is_some()
        {
            return self.any_atomic_type;
        }
        self.xpath_type(base_code)
    }

    /// Check if `derived` derives from `base` (transitively).
    ///
    /// Returns `true` if:
    /// - `derived == base`, or
    /// - `derived` has `base` somewhere in its derivation chain
    ///
    /// In an XSD 1.0 set every atomic built-in type derives from the data
    /// model's `xs:anyAtomicType` (XDM 1.0 §2.6.2: "All primitive atomic
    /// types … have xs:anyAtomicType as their base type"), although its
    /// {base type definition} chain ends at `xs:anySimpleType`.
    pub fn derives_from(&self, derived: SimpleTypeKey, base: SimpleTypeKey) -> bool {
        if derived == base {
            return true;
        }
        if self.is_detached_any_atomic_type(base) {
            return self
                .get_type_code(derived)
                .is_some_and(|code| code.is_atomic());
        }

        // Walk up the derivation chain
        let mut current = derived;
        while let Some(parent) = self.get_base_type(current) {
            if parent == base {
                return true;
            }
            current = parent;
        }

        false
    }

    /// Returns the number of built-in simple types the set has by code
    /// ([`get_by_type_code`](Self::get_by_type_code)): 45 for XSD 1.0, 51 for
    /// XSD 1.1 (its 50 and `xs:untypedAtomic`; without `xs:anyType`, and
    /// without the data model's types an XSD 1.0 set has for a host).
    pub fn count(&self) -> usize {
        self.by_type_code.len()
    }
}

/// Get the base type code for a built-in type.
///
/// Returns the immediate parent in the XSD derivation hierarchy.
fn get_builtin_base_type(code: XmlTypeCode) -> Option<XmlTypeCode> {
    match code {
        // anySimpleType has no base (it's the root of simple types)
        XmlTypeCode::AnySimpleType => None,

        // anyAtomicType derives from anySimpleType
        XmlTypeCode::AnyAtomicType => Some(XmlTypeCode::AnySimpleType),

        // All primitives derive from anyAtomicType (conceptually)
        // But for XSD 1.0, they derive from anySimpleType
        XmlTypeCode::String
        | XmlTypeCode::Boolean
        | XmlTypeCode::Decimal
        | XmlTypeCode::Float
        | XmlTypeCode::Double
        | XmlTypeCode::Duration
        | XmlTypeCode::DateTime
        | XmlTypeCode::Time
        | XmlTypeCode::Date
        | XmlTypeCode::GYearMonth
        | XmlTypeCode::GYear
        | XmlTypeCode::GMonthDay
        | XmlTypeCode::GDay
        | XmlTypeCode::GMonth
        | XmlTypeCode::HexBinary
        | XmlTypeCode::Base64Binary
        | XmlTypeCode::AnyUri
        | XmlTypeCode::QName
        | XmlTypeCode::Notation => Some(XmlTypeCode::AnySimpleType),

        // String-derived types
        XmlTypeCode::NormalizedString => Some(XmlTypeCode::String),
        XmlTypeCode::Token => Some(XmlTypeCode::NormalizedString),
        XmlTypeCode::Language => Some(XmlTypeCode::Token),
        XmlTypeCode::NmToken => Some(XmlTypeCode::Token),
        XmlTypeCode::Name => Some(XmlTypeCode::Token),
        XmlTypeCode::NCName => Some(XmlTypeCode::Name),
        XmlTypeCode::Id => Some(XmlTypeCode::NCName),
        XmlTypeCode::IdRef => Some(XmlTypeCode::NCName),
        XmlTypeCode::Entity => Some(XmlTypeCode::NCName),

        // Decimal-derived types (integer hierarchy)
        XmlTypeCode::Integer => Some(XmlTypeCode::Decimal),
        XmlTypeCode::NonPositiveInteger => Some(XmlTypeCode::Integer),
        XmlTypeCode::NegativeInteger => Some(XmlTypeCode::NonPositiveInteger),
        XmlTypeCode::Long => Some(XmlTypeCode::Integer),
        XmlTypeCode::Int => Some(XmlTypeCode::Long),
        XmlTypeCode::Short => Some(XmlTypeCode::Int),
        XmlTypeCode::Byte => Some(XmlTypeCode::Short),
        XmlTypeCode::NonNegativeInteger => Some(XmlTypeCode::Integer),
        XmlTypeCode::UnsignedLong => Some(XmlTypeCode::NonNegativeInteger),
        XmlTypeCode::UnsignedInt => Some(XmlTypeCode::UnsignedLong),
        XmlTypeCode::UnsignedShort => Some(XmlTypeCode::UnsignedInt),
        XmlTypeCode::UnsignedByte => Some(XmlTypeCode::UnsignedShort),
        XmlTypeCode::PositiveInteger => Some(XmlTypeCode::NonNegativeInteger),

        // XSD 1.1 types
        XmlTypeCode::UntypedAtomic => Some(XmlTypeCode::AnyAtomicType),
        XmlTypeCode::YearMonthDuration => Some(XmlTypeCode::Duration),
        XmlTypeCode::DayTimeDuration => Some(XmlTypeCode::Duration),
        XmlTypeCode::DateTimeStamp => Some(XmlTypeCode::DateTime),
        // xs:error is a union with no members; formally derives from xs:anySimpleType
        XmlTypeCode::Error => Some(XmlTypeCode::AnySimpleType),

        // List types derive from anySimpleType
        XmlTypeCode::NmTokens | XmlTypeCode::IdRefs | XmlTypeCode::Entities => {
            Some(XmlTypeCode::AnySimpleType)
        }

        // Non-simple types have no base in this hierarchy
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schema_set() -> SchemaSet {
        SchemaSet::new()
    }

    fn create_test_schema_set_v11() -> SchemaSet {
        SchemaSet::with_version(XsdVersion::V1_1)
    }

    #[test]
    fn test_builtin_types_creation() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        assert_eq!(builtin.count(), 45);
    }

    #[test]
    fn test_builtin_types_xsd11() {
        let mut schema_set = create_test_schema_set_v11();
        let builtin = BuiltinTypes::new(&mut schema_set);

        assert_eq!(builtin.count(), 51);

        // XSD 1.1 types should be present
        assert!(builtin.any_atomic_type.is_some());
        assert!(builtin.untyped_atomic.is_some());
        assert!(builtin.year_month_duration.is_some());
        assert!(builtin.day_time_duration.is_some());
        assert!(builtin.datetime_stamp.is_some());
    }

    #[test]
    fn test_builtin_types_xsd10_no_xsd11() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // XSD 1.1 types should not be present in XSD 1.0 mode
        assert!(builtin.any_atomic_type.is_none());
        assert!(builtin.untyped_atomic.is_none());
        assert!(builtin.year_month_duration.is_none());
        assert!(builtin.day_time_duration.is_none());
        assert!(builtin.datetime_stamp.is_none());
    }

    #[test]
    fn test_get_by_type_code() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        assert_eq!(
            builtin.get_by_type_code(XmlTypeCode::String),
            Some(builtin.string)
        );
        assert_eq!(
            builtin.get_by_type_code(XmlTypeCode::Integer),
            Some(builtin.integer)
        );
        assert_eq!(
            builtin.get_by_type_code(XmlTypeCode::DateTime),
            Some(builtin.datetime)
        );
        assert_eq!(
            builtin.get_by_type_code(XmlTypeCode::AnySimpleType),
            Some(builtin.any_simple_type)
        );

        // Node types should not be found
        assert!(builtin.get_by_type_code(XmlTypeCode::Element).is_none());
        assert!(builtin.get_by_type_code(XmlTypeCode::AnyType).is_none());
    }

    #[test]
    fn test_get_by_local_name() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        let string_id = schema_set.name_table.add("string");
        assert_eq!(builtin.get_by_local_name(string_id), Some(builtin.string));

        let integer_id = schema_set.name_table.add("integer");
        assert_eq!(builtin.get_by_local_name(integer_id), Some(builtin.integer));

        let nonexistent_id = schema_set.name_table.add("nonExistent");
        assert!(builtin.get_by_local_name(nonexistent_id).is_none());
    }

    #[test]
    fn test_get_type_code() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        assert_eq!(
            builtin.get_type_code(builtin.string),
            Some(XmlTypeCode::String)
        );
        assert_eq!(
            builtin.get_type_code(builtin.integer),
            Some(XmlTypeCode::Integer)
        );
        assert_eq!(
            builtin.get_type_code(builtin.any_simple_type),
            Some(XmlTypeCode::AnySimpleType)
        );
    }

    #[test]
    fn test_is_builtin() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        assert!(builtin.is_builtin(builtin.string));
        assert!(builtin.is_builtin(builtin.integer));
        assert!(builtin.is_builtin(builtin.any_simple_type));

        // Create a non-builtin type
        let custom_type = schema_set.arenas.alloc_simple_type(SimpleTypeDefData {
            name: Some(NameId(999)),
            target_namespace: None,
            variety: crate::parser::frames::SimpleTypeVariety::Atomic,
            base_type: None,
            item_type: None,
            member_types: Vec::new(),
            facets: crate::types::facets::FacetSet::new(),
            final_derivation: crate::schema::model::DerivationSet::empty(),
            id: None,
            derivation_id: None,
            annotation: None,
            source: None,
            // Resolved references
            resolved_base_type: None,
            resolved_item_type: None,
            resolved_member_types: Vec::new(),
            redefine_original: None,
            deferred_item_type_error: None,
        });
        assert!(!builtin.is_builtin(custom_type));
    }

    #[test]
    fn test_derives_from_same() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // A type derives from itself
        assert!(builtin.derives_from(builtin.string, builtin.string));
        assert!(builtin.derives_from(builtin.integer, builtin.integer));
    }

    #[test]
    fn test_derives_from_direct() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // Direct derivation
        assert!(builtin.derives_from(builtin.normalized_string, builtin.string));
        assert!(builtin.derives_from(builtin.integer, builtin.decimal));
        assert!(builtin.derives_from(builtin.long, builtin.integer));
    }

    #[test]
    fn test_derives_from_transitive() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // Transitive derivation: byte < short < int < long < integer < decimal
        assert!(builtin.derives_from(builtin.byte, builtin.decimal));
        assert!(builtin.derives_from(builtin.byte, builtin.integer));
        assert!(builtin.derives_from(builtin.byte, builtin.long));
        assert!(builtin.derives_from(builtin.int, builtin.decimal));

        // NCName < Name < token < normalizedString < string
        assert!(builtin.derives_from(builtin.ncname, builtin.string));
        assert!(builtin.derives_from(builtin.id, builtin.string));
    }

    #[test]
    fn test_derives_from_negative() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // Not derived
        assert!(!builtin.derives_from(builtin.string, builtin.integer));
        assert!(!builtin.derives_from(builtin.decimal, builtin.integer)); // Reverse
        assert!(!builtin.derives_from(builtin.float, builtin.double));
    }

    #[test]
    fn test_derives_from_any_simple_type() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // All simple types derive from anySimpleType
        assert!(builtin.derives_from(builtin.string, builtin.any_simple_type));
        assert!(builtin.derives_from(builtin.integer, builtin.any_simple_type));
        assert!(builtin.derives_from(builtin.byte, builtin.any_simple_type));
        assert!(builtin.derives_from(builtin.nmtokens, builtin.any_simple_type));
    }

    #[test]
    fn test_get_base_type() {
        let mut schema_set = create_test_schema_set();
        let builtin = BuiltinTypes::new(&mut schema_set);

        // anySimpleType has no base
        assert!(builtin.get_base_type(builtin.any_simple_type).is_none());

        // Primitives derive from anySimpleType
        assert_eq!(
            builtin.get_base_type(builtin.string),
            Some(builtin.any_simple_type)
        );

        // normalizedString derives from string
        assert_eq!(
            builtin.get_base_type(builtin.normalized_string),
            Some(builtin.string)
        );

        // integer derives from decimal
        assert_eq!(
            builtin.get_base_type(builtin.integer),
            Some(builtin.decimal)
        );
    }

    #[test]
    fn test_builtin_base_type_hierarchy() {
        // Test the get_builtin_base_type function directly
        assert_eq!(get_builtin_base_type(XmlTypeCode::AnySimpleType), None);
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::String),
            Some(XmlTypeCode::AnySimpleType)
        );
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::NormalizedString),
            Some(XmlTypeCode::String)
        );
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::Token),
            Some(XmlTypeCode::NormalizedString)
        );
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::NCName),
            Some(XmlTypeCode::Name)
        );
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::Integer),
            Some(XmlTypeCode::Decimal)
        );
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::Long),
            Some(XmlTypeCode::Integer)
        );
        assert_eq!(
            get_builtin_base_type(XmlTypeCode::Byte),
            Some(XmlTypeCode::Short)
        );
    }
}
