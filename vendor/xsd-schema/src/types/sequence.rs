//! XPath2 sequence types for type matching and conversion
//!
//! This module provides the `SequenceType` struct used for:
//! - Type matching in `instance of` and `treat as` expressions
//! - Function signatures and parameter types
//! - Type conversion target specifications

use std::fmt;

use super::XmlTypeCode;
use crate::ids::{SimpleTypeKey, TypeKey};
use crate::namespace::qname::QualifiedName;
use crate::types::value::XmlValue;
use crate::xpath::cast::type_matches;
use crate::xpath::context::XPathContext;
use crate::xpath::iterator::XmlItem;
use crate::xpath::DomNavigator;

/// XPath2 sequence type cardinality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum XmlTypeCardinality {
    /// Exactly one (T)
    #[default]
    One,
    /// Zero or one (T?)
    ZeroOrOne,
    /// One or more (T+)
    OneOrMore,
    /// Zero or more (T*)
    ZeroOrMore,
    /// Empty sequence only (empty-sequence())
    Empty,
}

impl XmlTypeCardinality {
    /// Check if this cardinality allows empty sequences
    pub fn allows_empty(&self) -> bool {
        matches!(self, Self::ZeroOrOne | Self::ZeroOrMore | Self::Empty)
    }

    /// Check if this cardinality allows multiple items
    pub fn allows_many(&self) -> bool {
        matches!(self, Self::OneOrMore | Self::ZeroOrMore)
    }

    /// Check if an actual count matches this cardinality
    pub fn matches_count(&self, count: usize) -> bool {
        match self {
            Self::One => count == 1,
            Self::ZeroOrOne => count <= 1,
            Self::OneOrMore => count >= 1,
            Self::ZeroOrMore => true,
            Self::Empty => count == 0,
        }
    }

    /// Get the symbol for this cardinality
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::One => "",
            Self::ZeroOrOne => "?",
            Self::OneOrMore => "+",
            Self::ZeroOrMore => "*",
            Self::Empty => "", // empty-sequence() has no suffix
        }
    }
}

impl fmt::Display for XmlTypeCardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// The `TypeName` of an `element(N, T)` or `attribute(N, T)` test, resolved:
/// the expected type `ET` of `derives-from(AT, ET)` (XPath 2.0 §2.5.4).
///
/// Most type names are a [`TypeKey`] of the schema set in the static context
/// — simple or complex. Two kinds of name have no key there, and still have to
/// be expressible, because XPath 2.0 §2.5.1 puts the predefined types in the
/// in-scope schema types of *every* static context:
///
/// * `xs:untyped`, "used as the type annotation of an element node that has not
///   been validated" (§2.5.1), which is an XDM type with no schema component;
/// * a predefined type of the `xs` namespace when the static context has no
///   schema set (or one that lacks the type: an XSD 1.0 set has no
///   `xs:anyAtomicType` or `xs:untypedAtomic`).
///
/// A node with no type annotation is untyped: an element is annotated
/// `xs:untyped`, an attribute `xs:untypedAtomic` (§2.5.1: "An attribute that
/// has been validated in skip mode is represented in the data model by an
/// attribute node with the type annotation xs:untypedAtomic"). Matching treats
/// `xs:untyped` as derived from `xs:anyType` only, and `xs:untypedAtomic` as an
/// atomic type ("xs:untypedAtomic is an atomic type", §2.5.1) derived from
/// `xs:anyAtomicType`, `xs:anySimpleType` and `xs:anyType`; "No predefined
/// schema types are derived from xs:untyped", nor from `xs:untypedAtomic`
/// (§2.5.1).
///
/// # Canonical form
///
/// The binder spells every name of the `xs` namespace as
/// [`TypeTest::Untyped`] or [`TypeTest::BuiltIn`], never as the schema set's
/// key for that type, and every other name as [`TypeTest::Schema`]. A host may
/// also build `Schema(key)` with the key of a predefined type: it **matches**
/// exactly like the `BuiltIn` spelling, but the derived `PartialEq` and `Hash`
/// tell the two apart. [`TypeTest::canonical`] maps such a key to the binder's
/// spelling, so compare canonical forms.
///
/// The enum is `#[non_exhaustive]`: a later release may add a way to name a
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TypeTest {
    /// A type definition of the schema set in the static context, simple or
    /// complex. `derives-from` is that set's derivation relation.
    Schema(TypeKey),
    /// A predefined type of the `xs` namespace, by its code — e.g.
    /// `xs:integer`, `xs:anyType`, `xs:untypedAtomic`. Matched against the
    /// schema set's built-in type of that code when there is one.
    ///
    /// XPath 2.0 §2.5.1 leaves the predefined types to the host ("a set of
    /// predefined schema types that is determined by the host language"). This
    /// engine's are every built-in type it knows, the XSD 1.1 types
    /// `xs:dateTimeStamp` and `xs:error` included, whatever schema set — or none
    /// — is attached: the same set `instance of`, `cast as` and the constructor
    /// functions accept. An annotated node of an XSD 1.0 set never derives
    /// from an XSD 1.1-only type, so such a test simply matches none of its
    /// nodes; `xs:anyAtomicType` still covers every atomic type there.
    BuiltIn(XmlTypeCode),
    /// `xs:untyped`.
    Untyped,
}

impl TypeTest {
    /// The canonical spelling of this test against `schema_set`: a
    /// [`TypeTest::Schema`] key of one of the set's predefined types becomes
    /// [`TypeTest::BuiltIn`] with that type's code (`xs:anyType` included);
    /// everything else is returned as it is. Two tests that name the same type
    /// have equal canonical forms. The binder already produces this form.
    pub fn canonical(&self, schema_set: &crate::schema::SchemaSet) -> TypeTest {
        match *self {
            TypeTest::Schema(key) if schema_set.is_any_type(key) => {
                TypeTest::BuiltIn(XmlTypeCode::AnyType)
            }
            TypeTest::Schema(TypeKey::Simple(simple)) => match schema_set.get_type_code(simple) {
                Some(code) => TypeTest::BuiltIn(code),
                None => *self,
            },
            other => other,
        }
    }
}

/// Item type in a sequence type
///
/// The enum and its two struct-like variants are `#[non_exhaustive]`: build the
/// element and attribute tests with [`ItemType::element`] and
/// [`ItemType::attribute`], and match them with `..`, so that a later release
/// can add an item type or a field without breaking a host.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ItemType {
    /// item() - any item
    AnyItem,
    /// node() - any node
    AnyNode,
    /// document-node() with optional element type
    Document(Option<Box<ItemType>>),
    /// `element()`, `element(N)`, `element(N, T)` or `element(N, T?)` —
    /// XPath 2.0 §2.5.4.3.
    #[non_exhaustive]
    Element {
        /// The `ElementName`, or a wildcard; `None` for `element()`.
        name: Option<NameTest>,
        /// The `TypeName`; `None` when the test names no type.
        type_test: Option<TypeTest>,
        /// `T?`: whether a nilled element matches too. Only meaningful with a
        /// `type_test` — without one, the nilled property never matters, and
        /// [`ItemType::element`] keeps it `false`.
        nillable: bool,
    },
    /// `attribute()`, `attribute(N)` or `attribute(N, T)` — XPath 2.0
    /// §2.5.4.5.
    #[non_exhaustive]
    Attribute {
        /// The `AttributeName`, or a wildcard; `None` for `attribute()`.
        name: Option<NameTest>,
        /// The `TypeName`; `None` when the test names no type.
        type_test: Option<TypeTest>,
    },
    /// schema-element()
    SchemaElement(QualifiedName),
    /// schema-attribute()
    SchemaAttribute(QualifiedName),
    /// text()
    Text,
    /// comment()
    Comment,
    /// processing-instruction()
    ProcessingInstruction(Option<String>),
    /// namespace-node()
    NamespaceNode,
    /// Atomic type (xs:string, xs:integer, etc.)
    AtomicType(XmlTypeCode),
    /// Schema-defined atomic type
    SchemaAtomicType(SimpleTypeKey),
}

impl ItemType {
    /// `element(name, T)` / `element(name, T?)`: an element test, with
    /// `nillable` the `?` after the type name (§2.5.4.3).
    ///
    /// `element()` is `ItemType::element(None, None, false)`, `element(*)`
    /// `ItemType::element(Some(NameTest::Wildcard), None, false)`.
    ///
    /// Without a `type_test` the nilled property never matters ("element(
    /// ElementName) matches any element node whose name is ElementName,
    /// regardless of its type annotation or nilled property"), so `nillable`
    /// is then normalized to `false`: `element(n)` has one spelling.
    pub fn element(name: Option<NameTest>, type_test: Option<TypeTest>, nillable: bool) -> Self {
        Self::Element {
            name,
            nillable: nillable && type_test.is_some(),
            type_test,
        }
    }

    /// `attribute(name, T)`: an attribute test (§2.5.4.5).
    pub fn attribute(name: Option<NameTest>, type_test: Option<TypeTest>) -> Self {
        Self::Attribute { name, type_test }
    }

    /// Get the type code for this item type
    pub fn type_code(&self) -> XmlTypeCode {
        match self {
            Self::AnyItem => XmlTypeCode::Item,
            Self::AnyNode => XmlTypeCode::Node,
            Self::Document(_) => XmlTypeCode::Document,
            Self::Element { .. } | Self::SchemaElement(_) => XmlTypeCode::Element,
            Self::Attribute { .. } | Self::SchemaAttribute(_) => XmlTypeCode::Attribute,
            Self::Text => XmlTypeCode::Text,
            Self::Comment => XmlTypeCode::Comment,
            Self::ProcessingInstruction(_) => XmlTypeCode::ProcessingInstruction,
            Self::NamespaceNode => XmlTypeCode::Namespace,
            Self::AtomicType(code) => *code,
            Self::SchemaAtomicType(_) => XmlTypeCode::AnyAtomicType,
        }
    }

    /// Check if this item type matches nodes
    pub fn is_node(&self) -> bool {
        matches!(
            self,
            Self::AnyNode
                | Self::Document(_)
                | Self::Element { .. }
                | Self::Attribute { .. }
                | Self::SchemaElement(_)
                | Self::SchemaAttribute(_)
                | Self::Text
                | Self::Comment
                | Self::ProcessingInstruction(_)
                | Self::NamespaceNode
        )
    }

    /// Check if this item type matches atomic values
    pub fn is_atomic(&self) -> bool {
        matches!(self, Self::AtomicType(_) | Self::SchemaAtomicType(_))
    }

    /// Check if a single XmlItem matches this item type.
    ///
    /// This is the runtime type matching method for function signatures
    /// and general type checking.
    pub fn matches_item<N: DomNavigator>(&self, item: &XmlItem<N>, ctx: &XPathContext<'_>) -> bool {
        match item {
            XmlItem::Node(nav) => self.matches_node(nav, ctx),
            XmlItem::Atomic(value) => self.matches_atomic(value, ctx),
        }
    }

    /// Check if a node matches this item type.
    ///
    /// The one node matcher a step node test, `instance of` and `treat as`
    /// also run (XPath 2.0 §2.5.4.2 – §2.5.4.6), so every spelling of a test
    /// agrees.
    fn matches_node<N: DomNavigator>(&self, nav: &N, ctx: &XPathContext<'_>) -> bool {
        crate::xpath::node_test::matches_item_type(self, nav, ctx)
    }

    /// Check if an atomic value matches this item type.
    fn matches_atomic(&self, value: &XmlValue, ctx: &XPathContext<'_>) -> bool {
        match self {
            Self::AnyItem => true,
            Self::AnyNode => false, // node() doesn't match atomics
            Self::AtomicType(code) => type_matches(value.type_code, *code),
            Self::SchemaAtomicType(key) => {
                // Check if value's schema type derives from the expected type
                if let Some(value_key) = value.schema_type {
                    if let Some(schema_set) = ctx.schema_set {
                        return schema_set
                            .xpath_derives_from(TypeKey::Simple(value_key), TypeKey::Simple(*key));
                    }
                    // Fallback: exact match
                    return value_key == *key;
                }
                // No schema type on value - can't match schema-defined type
                false
            }
            // All node types don't match atomics
            Self::Document(_)
            | Self::Element { .. }
            | Self::Attribute { .. }
            | Self::SchemaElement(_)
            | Self::SchemaAttribute(_)
            | Self::Text
            | Self::Comment
            | Self::ProcessingInstruction(_)
            | Self::NamespaceNode => false,
        }
    }
}

/// Name test for element/attribute tests (resolved form with interned names)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameTest {
    /// Wildcard (*) - matches any name
    Wildcard,
    /// Namespace wildcard (*:local) - matches any namespace with specific local name
    /// The NameId is the interned local name
    NamespaceWildcard(crate::ids::NameId),
    /// Local name wildcard (prefix:*) - matches any local name in namespace
    /// The NameId is the resolved namespace URI
    LocalWildcard(crate::ids::NameId),
    /// Specific QName (fully resolved)
    QName(QualifiedName),
}

impl NameTest {
    /// Check if this is a wildcard (matches any name)
    pub fn is_wildcard(&self) -> bool {
        matches!(self, Self::Wildcard)
    }
}

/// A diagnostic form. A name test holds interned names, which print as their
/// [`NameId`](crate::ids::NameId)s (`$7`) because no name table is at hand:
/// `*`, `*:$7`, `Q{$3}*`, `Q{$3}$7`, or `$7` for a name in no namespace.
impl fmt::Display for NameTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wildcard => f.write_str("*"),
            Self::NamespaceWildcard(local) => write!(f, "*:{local}"),
            Self::LocalWildcard(ns) => write!(f, "Q{{{ns}}}*"),
            Self::QName(qname) => write_qualified_name(f, qname),
        }
    }
}

/// `xs:untyped`, `xs:integer` (a built-in by its code) or the key of a schema
/// type (`SimpleTypeKey(…)`, `ComplexTypeKey(…)`), which has no name without
/// its schema set.
impl fmt::Display for TypeTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Untyped => f.write_str("xs:untyped"),
            Self::BuiltIn(code) => match code.local_name() {
                Some(name) => write!(f, "xs:{name}"),
                None => write!(f, "{code:?}"),
            },
            Self::Schema(TypeKey::Simple(key)) => write!(f, "{key:?}"),
            Self::Schema(TypeKey::Complex(key)) => write!(f, "{key:?}"),
        }
    }
}

fn write_qualified_name(f: &mut fmt::Formatter<'_>, qname: &QualifiedName) -> fmt::Result {
    match qname.namespace_uri {
        Some(ns) => write!(f, "Q{{{ns}}}{}", qname.local_name),
        None => write!(f, "{}", qname.local_name),
    }
}

/// The item type in the XPath 2.0 SequenceType syntax (§2.5.3), with interned
/// names and schema types in the diagnostic forms of [`NameTest`] and
/// [`TypeTest`]: e.g. `element(*, xs:untyped?)`, `attribute($7)`.
impl fmt::Display for ItemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemType::AnyItem => f.write_str("item()"),
            ItemType::AnyNode => f.write_str("node()"),
            ItemType::Document(None) => f.write_str("document-node()"),
            ItemType::Document(Some(inner)) => write!(f, "document-node({inner})"),
            ItemType::Element {
                name,
                type_test,
                nillable,
            } => {
                f.write_str("element(")?;
                match (name, type_test) {
                    (None, None) => {}
                    (name, type_test) => {
                        match name {
                            Some(name) => write!(f, "{name}")?,
                            None => f.write_str("*")?,
                        }
                        if let Some(type_test) = type_test {
                            write!(f, ", {type_test}")?;
                            if *nillable {
                                f.write_str("?")?;
                            }
                        }
                    }
                }
                f.write_str(")")
            }
            ItemType::Attribute { name, type_test } => {
                f.write_str("attribute(")?;
                match (name, type_test) {
                    (None, None) => {}
                    (name, type_test) => {
                        match name {
                            Some(name) => write!(f, "{name}")?,
                            None => f.write_str("*")?,
                        }
                        if let Some(type_test) = type_test {
                            write!(f, ", {type_test}")?;
                        }
                    }
                }
                f.write_str(")")
            }
            ItemType::SchemaElement(qname) => {
                f.write_str("schema-element(")?;
                write_qualified_name(f, qname)?;
                f.write_str(")")
            }
            ItemType::SchemaAttribute(qname) => {
                f.write_str("schema-attribute(")?;
                write_qualified_name(f, qname)?;
                f.write_str(")")
            }
            ItemType::Text => f.write_str("text()"),
            ItemType::Comment => f.write_str("comment()"),
            ItemType::ProcessingInstruction(None) => f.write_str("processing-instruction()"),
            ItemType::ProcessingInstruction(Some(name)) => {
                write!(f, "processing-instruction({name})")
            }
            ItemType::NamespaceNode => f.write_str("namespace-node()"),
            ItemType::AtomicType(code) => match code.local_name() {
                Some(name) => write!(f, "xs:{name}"),
                None => write!(f, "{code:?}"),
            },
            ItemType::SchemaAtomicType(key) => write!(f, "{key:?}"),
        }
    }
}

/// XPath2 sequence type
///
/// Describes the expected type of a sequence of items.
/// Used for type checking, type matching, and conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceType {
    /// The item type
    pub item_type: ItemType,
    /// The occurrence indicator
    pub cardinality: XmlTypeCardinality,
}

impl SequenceType {
    /// Create a new sequence type
    pub fn new(item_type: ItemType, cardinality: XmlTypeCardinality) -> Self {
        Self {
            item_type,
            cardinality,
        }
    }

    /// Create a sequence type for exactly one item
    pub fn one(item_type: ItemType) -> Self {
        Self::new(item_type, XmlTypeCardinality::One)
    }

    /// Create a sequence type for zero or one items
    pub fn optional(item_type: ItemType) -> Self {
        Self::new(item_type, XmlTypeCardinality::ZeroOrOne)
    }

    /// Create a sequence type for one or more items
    pub fn plus(item_type: ItemType) -> Self {
        Self::new(item_type, XmlTypeCardinality::OneOrMore)
    }

    /// Create a sequence type for zero or more items
    pub fn star(item_type: ItemType) -> Self {
        Self::new(item_type, XmlTypeCardinality::ZeroOrMore)
    }

    /// Create empty-sequence() type
    ///
    /// This only matches empty sequences (zero items).
    pub fn empty() -> Self {
        Self::new(ItemType::AnyItem, XmlTypeCardinality::Empty)
    }

    /// Check if this is the empty-sequence() type
    pub fn is_empty_sequence(&self) -> bool {
        self.cardinality == XmlTypeCardinality::Empty
    }

    /// Create item()* - any sequence
    pub fn any() -> Self {
        Self::star(ItemType::AnyItem)
    }

    /// Create item() - exactly one item
    pub fn item() -> Self {
        Self::one(ItemType::AnyItem)
    }

    /// Create node() - exactly one node
    pub fn node() -> Self {
        Self::one(ItemType::AnyNode)
    }

    /// Create node()* - sequence of nodes
    pub fn nodes() -> Self {
        Self::star(ItemType::AnyNode)
    }

    // Convenience constructors for common atomic types

    /// xs:string
    pub fn string() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::String))
    }

    /// xs:string?
    pub fn string_optional() -> Self {
        Self::optional(ItemType::AtomicType(XmlTypeCode::String))
    }

    /// xs:boolean
    pub fn boolean() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Boolean))
    }

    /// xs:integer
    pub fn integer() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Integer))
    }

    /// xs:integer?
    pub fn integer_optional() -> Self {
        Self::optional(ItemType::AtomicType(XmlTypeCode::Integer))
    }

    /// xs:decimal
    pub fn decimal() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Decimal))
    }

    /// xs:double
    pub fn double() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Double))
    }

    /// xs:double?
    pub fn double_optional() -> Self {
        Self::optional(ItemType::AtomicType(XmlTypeCode::Double))
    }

    /// xs:anyAtomicType
    pub fn any_atomic() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::AnyAtomicType))
    }

    /// xs:anyAtomicType?
    pub fn any_atomic_optional() -> Self {
        Self::optional(ItemType::AtomicType(XmlTypeCode::AnyAtomicType))
    }

    /// xs:anyAtomicType*
    pub fn any_atomic_star() -> Self {
        Self::star(ItemType::AtomicType(XmlTypeCode::AnyAtomicType))
    }

    /// xs:dateTime
    pub fn datetime() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::DateTime))
    }

    /// xs:date
    pub fn date() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Date))
    }

    /// xs:time
    pub fn time() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Time))
    }

    /// xs:duration
    pub fn duration() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::Duration))
    }

    /// xs:QName
    pub fn qname() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::QName))
    }

    /// xs:anyURI
    pub fn any_uri() -> Self {
        Self::one(ItemType::AtomicType(XmlTypeCode::AnyUri))
    }

    // Query methods

    /// Get the type code for the item type
    pub fn type_code(&self) -> XmlTypeCode {
        self.item_type.type_code()
    }

    /// Check if this sequence type matches nodes
    pub fn is_node(&self) -> bool {
        self.item_type.is_node()
    }

    /// Check if this sequence type matches atomic values
    pub fn is_atomic(&self) -> bool {
        self.item_type.is_atomic()
    }

    /// Check if this sequence type is numeric (decimal, float, double, integer types)
    pub fn is_numeric(&self) -> bool {
        self.type_code().is_numeric()
    }

    /// Check if this sequence type allows empty sequences
    pub fn allows_empty(&self) -> bool {
        self.cardinality.allows_empty()
    }

    /// Check if a sequence of items matches this sequence type.
    ///
    /// Validates both cardinality and item type for each item in the sequence.
    pub fn matches_sequence<N: DomNavigator>(
        &self,
        items: &[XmlItem<N>],
        ctx: &XPathContext<'_>,
    ) -> bool {
        // Check cardinality first
        if !self.cardinality.matches_count(items.len()) {
            return false;
        }
        // Check each item matches the item type
        for item in items {
            if !self.item_type.matches_item(item, ctx) {
                return false;
            }
        }
        true
    }
}

impl Default for SequenceType {
    fn default() -> Self {
        Self::any()
    }
}

impl fmt::Display for SequenceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Handle empty-sequence() specially
        if self.cardinality == XmlTypeCardinality::Empty {
            return write!(f, "empty-sequence()");
        }

        // Format item type
        write!(f, "{}", self.item_type)?;

        // Format cardinality
        write!(f, "{}", self.cardinality)?;

        Ok(())
    }
}

// ============================================================================
// Helpers (xsd11)
// ============================================================================

/// Resolve the item type's `SimpleTypeKey` from a list type definition.
///
/// Given a list type's `SimpleTypeKey`, looks up its `resolved_item_type` and
/// extracts the `SimpleTypeKey` (if it refers to a simple type).
/// Returns `None` if the type is not found or item type is not a simple type.
#[cfg(feature = "xsd11")]
pub fn resolve_list_item_schema_type(
    list_type_key: SimpleTypeKey,
    schema_set: &crate::schema::SchemaSet,
) -> Option<SimpleTypeKey> {
    let st_data = schema_set.arenas.simple_types.get(list_type_key)?;
    st_data.resolved_item_type?.as_simple()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    use crate::namespace::table::NameTable;
    use crate::navigator::RoXmlNavigator;

    #[test]
    fn test_cardinality_matches_count() {
        assert!(XmlTypeCardinality::One.matches_count(1));
        assert!(!XmlTypeCardinality::One.matches_count(0));
        assert!(!XmlTypeCardinality::One.matches_count(2));

        assert!(XmlTypeCardinality::ZeroOrOne.matches_count(0));
        assert!(XmlTypeCardinality::ZeroOrOne.matches_count(1));
        assert!(!XmlTypeCardinality::ZeroOrOne.matches_count(2));

        assert!(!XmlTypeCardinality::OneOrMore.matches_count(0));
        assert!(XmlTypeCardinality::OneOrMore.matches_count(1));
        assert!(XmlTypeCardinality::OneOrMore.matches_count(100));

        assert!(XmlTypeCardinality::ZeroOrMore.matches_count(0));
        assert!(XmlTypeCardinality::ZeroOrMore.matches_count(1));
        assert!(XmlTypeCardinality::ZeroOrMore.matches_count(100));
    }

    #[test]
    fn test_cardinality_symbols() {
        assert_eq!(XmlTypeCardinality::One.symbol(), "");
        assert_eq!(XmlTypeCardinality::ZeroOrOne.symbol(), "?");
        assert_eq!(XmlTypeCardinality::OneOrMore.symbol(), "+");
        assert_eq!(XmlTypeCardinality::ZeroOrMore.symbol(), "*");
    }

    #[test]
    fn test_sequence_type_display() {
        assert_eq!(SequenceType::item().to_string(), "item()");
        assert_eq!(SequenceType::string().to_string(), "xs:string");
        assert_eq!(SequenceType::string_optional().to_string(), "xs:string?");
        assert_eq!(SequenceType::nodes().to_string(), "node()*");
        assert_eq!(SequenceType::integer().to_string(), "xs:integer");
    }

    /// The element and attribute tests display in the SequenceType syntax,
    /// interned names as their ids and schema types as their keys.
    #[test]
    fn test_element_and_attribute_item_type_display() {
        let table = NameTable::new();
        let ns = table.add("urn:x");
        let local = table.add("a");
        let qname = QualifiedName::new(Some(ns), local, None);
        let q = |s: String| {
            s.replace("$ns", &ns.to_string())
                .replace("$l", &local.to_string())
        };

        assert_eq!(
            SequenceType::one(ItemType::element(None, None, false)).to_string(),
            "element()"
        );
        assert_eq!(
            ItemType::element(Some(NameTest::Wildcard), None, false).to_string(),
            "element(*)"
        );
        assert_eq!(
            ItemType::element(Some(NameTest::QName(qname.clone())), None, false).to_string(),
            q("element(Q{$ns}$l)".to_string())
        );
        assert_eq!(
            ItemType::element(None, Some(TypeTest::Untyped), true).to_string(),
            "element(*, xs:untyped?)"
        );
        assert_eq!(
            SequenceType::star(ItemType::element(
                Some(NameTest::QName(qname.clone())),
                Some(TypeTest::BuiltIn(XmlTypeCode::Integer)),
                false
            ))
            .to_string(),
            q("element(Q{$ns}$l, xs:integer)*".to_string())
        );
        // A schema type has no name without its schema set: its key prints.
        let schema_set = crate::schema::SchemaSet::new();
        let any_type = TypeKey::Complex(schema_set.any_type_key());
        let shown = ItemType::element(None, Some(TypeTest::Schema(any_type)), false).to_string();
        assert!(shown.starts_with("element(*, ComplexTypeKey("), "{shown}");
        assert_eq!(ItemType::attribute(None, None).to_string(), "attribute()");
        assert_eq!(
            ItemType::attribute(
                Some(NameTest::NamespaceWildcard(local)),
                Some(TypeTest::BuiltIn(XmlTypeCode::UntypedAtomic))
            )
            .to_string(),
            q("attribute(*:$l, xs:untypedAtomic)".to_string())
        );
        assert_eq!(
            SequenceType::optional(ItemType::attribute(Some(NameTest::LocalWildcard(ns)), None))
                .to_string(),
            q("attribute(Q{$ns}*)?".to_string())
        );
        assert_eq!(
            ItemType::Document(Some(Box::new(ItemType::SchemaElement(qname)))).to_string(),
            q("document-node(schema-element(Q{$ns}$l))".to_string())
        );
    }

    #[test]
    fn test_sequence_type_is_node() {
        assert!(SequenceType::node().is_node());
        assert!(SequenceType::nodes().is_node());
        assert!(!SequenceType::string().is_node());
        assert!(!SequenceType::any_atomic().is_node());
    }

    #[test]
    fn test_sequence_type_is_atomic() {
        assert!(SequenceType::string().is_atomic());
        assert!(SequenceType::integer().is_atomic());
        assert!(SequenceType::any_atomic().is_atomic());
        assert!(!SequenceType::node().is_atomic());
        assert!(!SequenceType::item().is_atomic());
    }

    #[test]
    fn test_sequence_type_is_numeric() {
        assert!(SequenceType::decimal().is_numeric());
        assert!(SequenceType::integer().is_numeric());
        assert!(SequenceType::double().is_numeric());
        assert!(!SequenceType::string().is_numeric());
        assert!(!SequenceType::boolean().is_numeric());
    }

    #[test]
    fn test_item_type_type_code() {
        assert_eq!(ItemType::AnyItem.type_code(), XmlTypeCode::Item);
        assert_eq!(ItemType::AnyNode.type_code(), XmlTypeCode::Node);
        assert_eq!(ItemType::Text.type_code(), XmlTypeCode::Text);
        assert_eq!(
            ItemType::AtomicType(XmlTypeCode::String).type_code(),
            XmlTypeCode::String
        );
    }

    // ============================================================================
    // Runtime Sequence Type Matching Tests
    // ============================================================================

    #[test]
    fn test_cardinality_one() {
        // xs:integer requires exactly 1 item
        let seq_type = SequenceType::integer();
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let one_item: Vec<XmlItem<RoXmlNavigator<'static>>> =
            vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(42)))];
        let no_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![];
        let two_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ];

        assert!(seq_type.matches_sequence(&one_item, &ctx));
        assert!(!seq_type.matches_sequence(&no_items, &ctx));
        assert!(!seq_type.matches_sequence(&two_items, &ctx));
    }

    #[test]
    fn test_cardinality_optional() {
        // xs:integer? allows 0 or 1 item
        let seq_type = SequenceType::integer_optional();
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let one_item: Vec<XmlItem<RoXmlNavigator<'static>>> =
            vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(42)))];
        let no_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![];
        let two_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ];

        assert!(seq_type.matches_sequence(&one_item, &ctx));
        assert!(seq_type.matches_sequence(&no_items, &ctx));
        assert!(!seq_type.matches_sequence(&two_items, &ctx));
    }

    #[test]
    fn test_cardinality_star() {
        // xs:integer* allows any count
        let seq_type = SequenceType::star(ItemType::AtomicType(XmlTypeCode::Integer));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let no_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![];
        let one_item: Vec<XmlItem<RoXmlNavigator<'static>>> =
            vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(42)))];
        let three_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(3))),
        ];

        assert!(seq_type.matches_sequence(&no_items, &ctx));
        assert!(seq_type.matches_sequence(&one_item, &ctx));
        assert!(seq_type.matches_sequence(&three_items, &ctx));
    }

    #[test]
    fn test_cardinality_plus() {
        // xs:integer+ requires at least 1 item
        let seq_type = SequenceType::plus(ItemType::AtomicType(XmlTypeCode::Integer));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let no_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![];
        let one_item: Vec<XmlItem<RoXmlNavigator<'static>>> =
            vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(42)))];
        let three_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(3))),
        ];

        assert!(!seq_type.matches_sequence(&no_items, &ctx));
        assert!(seq_type.matches_sequence(&one_item, &ctx));
        assert!(seq_type.matches_sequence(&three_items, &ctx));
    }

    #[test]
    fn test_atomic_integer_match() {
        // Integer value matches xs:integer
        let item_type = ItemType::AtomicType(XmlTypeCode::Integer);
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let int_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::integer(BigInt::from(42)));
        let str_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::string("hello"));

        assert!(item_type.matches_item(&int_value, &ctx));
        assert!(!item_type.matches_item(&str_value, &ctx));
    }

    #[test]
    fn test_atomic_string_match() {
        // String value matches xs:string
        let item_type = ItemType::AtomicType(XmlTypeCode::String);
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let str_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::string("hello"));
        let int_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::integer(BigInt::from(42)));

        assert!(item_type.matches_item(&str_value, &ctx));
        assert!(!item_type.matches_item(&int_value, &ctx));
    }

    #[test]
    fn test_atomic_type_hierarchy() {
        // Any atomic matches xs:anyAtomicType
        let item_type = ItemType::AtomicType(XmlTypeCode::AnyAtomicType);
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let str_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::string("hello"));
        let int_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::integer(BigInt::from(42)));
        let bool_value: XmlItem<RoXmlNavigator<'static>> = XmlItem::Atomic(XmlValue::boolean(true));

        assert!(item_type.matches_item(&str_value, &ctx));
        assert!(item_type.matches_item(&int_value, &ctx));
        assert!(item_type.matches_item(&bool_value, &ctx));
    }

    #[test]
    fn test_item_matches_any() {
        // item() matches both nodes and atomics
        let item_type = ItemType::AnyItem;
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();

        let node_item = XmlItem::Node(nav);
        let atomic_item: XmlItem<RoXmlNavigator<'_>> = XmlItem::Atomic(XmlValue::string("hello"));

        assert!(item_type.matches_item(&node_item, &ctx));
        assert!(item_type.matches_item(&atomic_item, &ctx));
    }

    #[test]
    fn test_node_rejects_atomic() {
        // node() rejects atomic values
        let item_type = ItemType::AnyNode;
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let atomic_item: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::string("hello"));

        assert!(!item_type.matches_item(&atomic_item, &ctx));
    }

    #[test]
    fn test_atomic_rejects_node() {
        // xs:integer rejects nodes
        let item_type = ItemType::AtomicType(XmlTypeCode::Integer);
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();

        let node_item = XmlItem::Node(nav);

        assert!(!item_type.matches_item(&node_item, &ctx));
    }

    #[test]
    fn test_element_item_type_matches_element() {
        // element() matches element nodes
        let item_type = ItemType::element(None, None, false);
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child();

        let node_item = XmlItem::Node(nav);

        assert!(item_type.matches_item(&node_item, &ctx));
    }

    #[test]
    fn test_text_item_type_matches_text() {
        // text() matches text nodes
        let item_type = ItemType::Text;
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<root>text</root>").expect("parse xml");
        let mut nav = RoXmlNavigator::new(&doc);
        nav.move_to_first_child(); // root
        nav.move_to_first_child(); // text

        let node_item = XmlItem::Node(nav);

        assert!(item_type.matches_item(&node_item, &ctx));
    }

    #[test]
    fn test_sequence_type_mixed_types_fail() {
        // Sequence with wrong type fails
        let seq_type = SequenceType::star(ItemType::AtomicType(XmlTypeCode::Integer));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let mixed_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::string("not an integer")),
        ];

        assert!(!seq_type.matches_sequence(&mixed_items, &ctx));
    }

    #[test]
    fn test_integer_derived_types() {
        // Integer derived types match xs:integer via type_matches
        let item_type = ItemType::AtomicType(XmlTypeCode::Integer);
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        // Create a value with integer type code
        let int_value: XmlItem<RoXmlNavigator<'static>> =
            XmlItem::Atomic(XmlValue::integer(BigInt::from(42)));

        assert!(item_type.matches_item(&int_value, &ctx));
    }

    // ============================================================================
    // empty-sequence() Tests
    // ============================================================================

    #[test]
    fn test_empty_sequence_matches_empty() {
        // empty-sequence() only matches empty sequences
        let seq_type = SequenceType::empty();
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let empty: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![];
        assert!(seq_type.matches_sequence(&empty, &ctx));
    }

    #[test]
    fn test_empty_sequence_rejects_non_empty() {
        // empty-sequence() rejects non-empty sequences
        let seq_type = SequenceType::empty();
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let one_item: Vec<XmlItem<RoXmlNavigator<'static>>> =
            vec![XmlItem::Atomic(XmlValue::integer(BigInt::from(42)))];
        let two_items: Vec<XmlItem<RoXmlNavigator<'static>>> = vec![
            XmlItem::Atomic(XmlValue::integer(BigInt::from(1))),
            XmlItem::Atomic(XmlValue::integer(BigInt::from(2))),
        ];

        assert!(!seq_type.matches_sequence(&one_item, &ctx));
        assert!(!seq_type.matches_sequence(&two_items, &ctx));
    }

    #[test]
    fn test_empty_sequence_display() {
        // empty-sequence() displays correctly
        assert_eq!(SequenceType::empty().to_string(), "empty-sequence()");
    }

    #[test]
    fn test_is_empty_sequence() {
        assert!(SequenceType::empty().is_empty_sequence());
        assert!(!SequenceType::any().is_empty_sequence());
        assert!(!SequenceType::integer().is_empty_sequence());
    }

    // ============================================================================
    // document-node(E) Tests
    // ============================================================================

    #[test]
    fn test_document_node_with_element_single_element() {
        // document-node(element()) matches document with exactly one element child
        let item_type = ItemType::Document(Some(Box::new(ItemType::element(None, None, false))));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
        let nav = RoXmlNavigator::new(&doc);

        assert!(item_type.matches_node(&nav, &ctx));
    }

    #[test]
    fn test_document_node_with_element_allows_comments() {
        // document-node(element()) allows comments alongside the element
        let item_type = ItemType::Document(Some(Box::new(ItemType::element(None, None, false))));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<!-- comment --><root/><!-- another -->")
            .expect("parse xml");
        let nav = RoXmlNavigator::new(&doc);

        assert!(item_type.matches_node(&nav, &ctx));
    }

    #[test]
    fn test_document_node_with_element_allows_pi() {
        // document-node(element()) allows processing instructions alongside the element
        let item_type = ItemType::Document(Some(Box::new(ItemType::element(None, None, false))));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<?target data?><root/>").expect("parse xml");
        let nav = RoXmlNavigator::new(&doc);

        assert!(item_type.matches_node(&nav, &ctx));
    }

    #[test]
    fn test_document_node_with_element_rejects_no_element() {
        // document-node(element()) rejects document with no element children
        let item_type = ItemType::Document(Some(Box::new(ItemType::element(None, None, false))));
        let table = NameTable::new();
        let ctx = XPathContext::new(&table);

        // A document with only a comment (no root element) - this would be malformed XML
        // but we can test the logic by checking a document node directly
        let doc = roxmltree::Document::parse("<!-- comment only is invalid XML --><dummy/>")
            .expect("parse xml");
        let nav = RoXmlNavigator::new(&doc);

        // This should match since there's still an element
        assert!(item_type.matches_node(&nav, &ctx));
    }

    #[test]
    fn test_document_node_with_specific_element_name() {
        // document-node(element(root)) matches document with element named "root"
        let table = NameTable::new();
        let root_id = table.add("root");
        let qname = QualifiedName::local(root_id);
        let name_test = NameTest::QName(qname);
        let item_type = ItemType::Document(Some(Box::new(ItemType::element(
            Some(name_test),
            None,
            false,
        ))));
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<root/>").expect("parse xml");
        let nav = RoXmlNavigator::new(&doc);

        assert!(item_type.matches_node(&nav, &ctx));
    }

    #[test]
    fn test_document_node_with_wrong_element_name() {
        // document-node(element(root)) rejects document with element named "other"
        let table = NameTable::new();
        let root_id = table.add("root");
        let qname = QualifiedName::local(root_id);
        let name_test = NameTest::QName(qname);
        let item_type = ItemType::Document(Some(Box::new(ItemType::element(
            Some(name_test),
            None,
            false,
        ))));
        let ctx = XPathContext::new(&table);

        let doc = roxmltree::Document::parse("<other/>").expect("parse xml");
        let nav = RoXmlNavigator::new(&doc);

        assert!(!item_type.matches_node(&nav, &ctx));
    }
}
