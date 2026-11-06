//! DOM navigation trait and types
//!
//! This module provides a read-only navigation interface for XML documents,
//! independent of the underlying DOM implementation.
//!
//! ## Core Types
//!
//! - [`DomNavigator`] - Trait for cursor-based XML navigation
//! - [`DomNodeType`] - Node type enumeration (Element, Text, Attribute, etc.)
//! - [`XmlNodeOrder`] - Document order comparison result
//! - [`NamespaceAxisScope`] - Scope filter for namespace axis traversal
//! - [`NavigatorError`] - Error type for navigator operations
//!
//! ## Adapters
//!
//! - [`RoXmlNavigator`] - Adapter for roxmltree backend (untyped)

pub mod roxmltree;

pub use self::roxmltree::RoXmlNavigator;

use std::borrow::Cow;

use crate::ids::{SimpleTypeKey, TypeKey};
use crate::types::value::XmlValue;

/// Error type for navigator operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum NavigatorError {
    #[error("{0}")]
    Other(String),
}

/// XDM typed-value result for a node.
///
/// Distinguishes the four states that the old `Option<XmlValue>` conflated:
///
/// | Variant   | XDM meaning                           |
/// |-----------|---------------------------------------|
/// | `Value`   | Schema-validated typed atomic value    |
/// | `Untyped` | No schema — atomizes to untypedAtomic |
/// | `Nilled`  | `xsi:nil="true"` — empty sequence     |
/// | `Absent`  | Element-only complex content (FOTY0012)|
///
/// A list type's typed value is one `Value` of kind
/// [`XmlValueKind::List`](crate::types::value::XmlValueKind::List), which XPath
/// atomization sees as one atomic value per member, each of the type that
/// validated it (for a list whose item type is a union, the member type that
/// accepted that member); an element whose complex type has empty content has
/// an empty list as its `Value` (XDM 1.0 §6.2.4: the empty sequence).
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    /// Schema-validated typed atomic value.
    Value(XmlValue),
    /// Untyped node (no schema), or an element whose complex type has mixed
    /// content, `xs:anyType` included (XDM 1.0 §6.2.4) — atomizes to its
    /// string value as `xs:untypedAtomic`.
    Untyped,
    /// Nilled element (`xsi:nil="true"`) — typed value is empty sequence.
    Nilled,
    /// No typed value (element-only complex content) — FOTY0012 on atomization.
    Absent,
}

/// XML node types for XPath navigation
///
/// Maps to XPathNodeType in the C# implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DomNodeType {
    /// Document root node
    Root,
    /// Element node
    Element,
    /// Attribute node
    Attribute,
    /// Namespace declaration node
    Namespace,
    /// Text node
    Text,
    /// Whitespace-only text node
    Whitespace,
    /// Significant whitespace text node
    SignificantWhitespace,
    /// Comment node
    Comment,
    /// Processing instruction node
    ProcessingInstruction,
    /// Wildcard - matches any node type (for traversal methods)
    All,
}

impl DomNodeType {
    /// Check if this is a text-like node type
    pub fn is_text_like(self) -> bool {
        matches!(
            self,
            DomNodeType::Text | DomNodeType::Whitespace | DomNodeType::SignificantWhitespace
        )
    }
}

/// Result of document order comparison between two nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlNodeOrder {
    /// First node comes before second in document order
    Before,
    /// First node comes after second in document order
    After,
    /// Nodes are the same position
    Same,
    /// Order cannot be determined (different documents)
    Unknown,
}

/// Scope filter for namespace axis traversal.
///
/// Contract shared by every [`DomNavigator`] backend (BufferDoc, roxmltree):
///
/// * `All` follows the **XDM data model** — it yields every in-scope namespace,
///   inherited declarations included, and **always** the implicit `xml:` binding
///   (even on an element with no declarations of its own). This is what the
///   `namespace::` axis needs. Use it only where XDM semantics are required;
///   it allocates at least the `xml:` node on every element.
/// * `ExcludeXml` is `All` minus the `xml:` binding (implicit or explicit). It is
///   the right scope for callers that treat `xml:` specially or discard it —
///   e.g. validation's namespace snapshot — and it returns nothing (heap-free)
///   on a namespace-free element.
/// * `Local` yields only the namespaces declared on the element itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceAxisScope {
    /// Every in-scope namespace (inherited included) plus the implicit `xml:`
    /// binding — the XDM `namespace::` axis contract.
    All,
    /// Only locally declared namespaces.
    Local,
    /// All in-scope namespaces *except* the `xml:` binding.
    ExcludeXml,
}

/// Read-only cursor-based navigator for XPath2 evaluation
///
/// Provides cursor movement and node information access.
/// Implementations must be `Clone` to support iterator branching.
pub trait DomNavigator: Clone {
    // ----- Node identity and order -----

    /// Check if this navigator is at the same position as another
    fn is_same_position(&self, other: &Self) -> bool;

    /// Compare document order of this position with another
    fn compare_position(&self, other: &Self) -> XmlNodeOrder;

    /// Move this navigator to the position of another
    fn move_to(&mut self, other: &Self) -> bool;

    // ----- Navigation -----

    /// Move to the document root
    fn move_to_root(&mut self);

    /// Move to the root of the **visible** tree. In normal scope this is the
    /// document root; in XSD 1.1 assertion scope it is the asserter element
    /// (the "fragment root"), whose ancestors are outside the tree the
    /// assertion sees. Axis iterators that have to bound a traversal at the
    /// top of the tree — `preceding`, which walks back towards it — use this
    /// rather than `move_to_root`, so they stay inside the asserted subtree
    /// instead of running into the synthetic root whose children are
    /// deliberately hidden by `move_to_first_child`.
    ///
    /// Default implementation calls `move_to_root`. Implementations that
    /// support assertion scope should override.
    fn move_to_visible_root(&mut self) {
        self.move_to_root();
    }

    /// Move to the parent node (returns false if at root)
    fn move_to_parent(&mut self) -> bool;

    /// Move to the first child (returns false if no children)
    fn move_to_first_child(&mut self) -> bool;

    /// Move to the next sibling (returns false if no next sibling)
    fn move_to_next_sibling(&mut self) -> bool;

    /// Move to the previous sibling (returns false if no prev sibling)
    fn move_to_prev_sibling(&mut self) -> bool;

    /// Move to the first attribute (returns false if no attributes)
    fn move_to_first_attribute(&mut self) -> bool;

    /// Move to the next attribute (returns false if no more attributes)
    fn move_to_next_attribute(&mut self) -> bool;

    /// Move to the first namespace in scope
    fn move_to_first_namespace(&mut self, scope: NamespaceAxisScope) -> bool;

    /// Move to the next namespace in scope
    fn move_to_next_namespace(&mut self, scope: NamespaceAxisScope) -> bool;

    /// Move to the next node in the **following axis** (excludes descendants).
    ///
    /// If `end` is provided, stop before reaching that position.
    /// On `false` the cursor position is unchanged.
    fn move_to_following(&mut self, kind: DomNodeType, end: Option<&Self>) -> bool;

    // ----- Node information -----

    /// Get the type of the current node
    fn node_type(&self) -> DomNodeType;

    /// Get the local name of the current node
    fn local_name(&self) -> &str;

    /// Get the qualified name (prefix:local) of the current node
    fn name(&self) -> &str;

    /// Get the namespace URI of the current node
    fn namespace_uri(&self) -> &str;

    /// Get the prefix of the current node
    fn prefix(&self) -> &str;

    /// Get the string value of the current node
    fn value(&self) -> String;

    /// Borrowed string value of the current node when the backing store can
    /// supply one without allocating; owned otherwise.
    ///
    /// The default delegates to [`value`](Self::value) (always owned), so existing
    /// implementors need no change. DOM backends whose text/attribute values are
    /// interned contiguously (e.g. `BufferDocNavigator`, `RoXmlNavigator`) override
    /// this to borrow, eliminating a per-value allocation in the validation walk.
    fn value_ref(&self) -> Cow<'_, str> {
        Cow::Owned(self.value())
    }

    /// The base URI this navigator knows for the current node, **unresolved**.
    ///
    /// What a navigator returns:
    ///
    /// * at the document node: the base URI of the document entity — the URI
    ///   the document was retrieved from, or was given when it was built — or
    ///   `""` when none is known. `xml:base` never changes it;
    /// * at any other node: either that same document base URI
    ///   ([`RoXmlNavigator`] reports it for every node), or the value of the
    ///   `xml:base` attribute in scope at the node, exactly as written
    ///   (`document::BufferDocNavigator` reports it where there is one) —
    ///   never a value resolved from the two;
    /// * at the topmost node of a tree without a document node (a fragment):
    ///   the document's base URI when the node carries no `xml:base` of its
    ///   own.
    ///
    /// Who resolves: the consumer. `fn:base-uri` takes this value at the
    /// document node (or at the topmost node of a fragment), resolves every
    /// `xml:base` from the outermost ancestor-or-self inward against it (XML
    /// Base; RFC 3986 §5.2, which removes dot segments, §5.2.4), and finally
    /// resolves the result against the static base URI when it is still
    /// relative. A navigator does not resolve the `xml:base` chain itself — a
    /// consumer that does so would apply it twice — and a host that needs the
    /// base URI of a node (to resolve a relative reference found in it)
    /// resolves it the same way.
    fn base_uri(&self) -> &str;

    // ----- Typed value hooks -----

    /// Get the schema type of the current node (if known)
    fn schema_type(&self) -> Option<SimpleTypeKey>;

    /// The **type annotation** of the current node, as a full [`TypeKey`].
    ///
    /// This is the node's XDM *type-name* property: the type the node was
    /// annotated with by schema validation. Unlike
    /// [`schema_type`](Self::schema_type) it is not restricted to simple
    /// types — an element validated against a complex type reports
    /// [`TypeKey::Complex`], which is what `element(*, T)` and
    /// `schema-element(x)` matching need in order to accept complex `T`.
    ///
    /// Returns:
    ///
    /// * `Some(TypeKey::Complex(_))` or `Some(TypeKey::Simple(_))` for an
    ///   **element** or **attribute** node carrying a type annotation;
    /// * `None` for an untyped node (no schema binding — e.g. any node of a
    ///   document that was never validated, or a lax-validated node that was
    ///   skipped);
    /// * `None` for every other node kind (root, text, comment,
    ///   processing-instruction, namespace), which have no type annotation in
    ///   the XDM sense.
    ///
    /// [`schema_type`](Self::schema_type) remains the *simple-type
    /// projection* of this value: where `type_annotation()` returns
    /// `Some(TypeKey::Simple(k))`, `schema_type()` returns `Some(k)`; where it
    /// returns `Some(TypeKey::Complex(_))`, `schema_type()` returns `None`.
    ///
    /// The default implementation returns `None`, which is correct for any
    /// schema-unaware backend (for example [`RoXmlNavigator`]); schema-aware
    /// navigators override it.
    fn type_annotation(&self) -> Option<TypeKey> {
        None
    }

    /// Get the typed value of the current node.
    ///
    /// Returns a [`TypedValue`] that distinguishes validated values, untyped
    /// nodes, nilled elements, and element-only complex content.
    fn typed_value(&self) -> TypedValue;

    /// The XDM is-idrefs property of the current node (XDM 1.0 §5.6):
    /// whether it is an element or attribute whose typed value holds an
    /// `xs:IDREF` / `xs:IDREFS` (or derived) atomic value (§6.2.4, §6.3.4).
    ///
    /// The default implementation returns `false`, which is correct for any
    /// schema-unaware backend: a node constructed from an infoset without DTD
    /// attribute types is never is-idrefs. Schema-aware navigators override
    /// it, together with [`find_nodes_by_idref`](Self::find_nodes_by_idref).
    fn is_idrefs(&self) -> bool {
        false
    }

    // ----- Default helper methods -----

    /// Check if the current element has attributes
    fn has_attributes(&mut self) -> bool {
        let ok = self.move_to_first_attribute();
        if ok {
            self.move_to_parent();
        }
        ok
    }

    /// Check if the current node has children
    fn has_children(&mut self) -> bool {
        let ok = self.move_to_first_child();
        if ok {
            self.move_to_parent();
        }
        ok
    }

    /// Move to the first child of the given node type
    fn move_to_child_kind(&mut self, kind: DomNodeType) -> bool {
        if self.move_to_first_child() {
            loop {
                if self.node_type() == kind || kind == DomNodeType::All {
                    return true;
                }
                if !self.move_to_next_sibling() {
                    break;
                }
            }
            self.move_to_parent();
        }
        false
    }

    /// Move to the first child element with the given name
    fn move_to_child_name(&mut self, local: &str, ns: &str) -> bool {
        if self.move_to_first_child() {
            loop {
                if self.node_type() == DomNodeType::Element
                    && self.local_name() == local
                    && self.namespace_uri() == ns
                {
                    return true;
                }
                if !self.move_to_next_sibling() {
                    break;
                }
            }
            self.move_to_parent();
        }
        false
    }

    /// Find an element by its ID attribute value.
    ///
    /// Returns `Ok(Some(navigator))` positioned at the matching element,
    /// `Ok(None)` if no element with this ID exists.
    ///
    /// The default implementation always returns `Ok(None)`, which is
    /// spec-compliant for documents without DTD/schema ID declarations.
    /// Schema-aware navigators should override this method.
    fn find_element_by_id(&self, id: &str) -> Result<Option<Self>, NavigatorError> {
        let _ = id;
        Ok(None)
    }

    /// The element and attribute nodes of this node's tree with the
    /// is-idrefs property whose string value has `id` among its
    /// whitespace-separated tokens — the nodes `fn:idref` returns for the
    /// candidate `id` (F&O 1.0 §15.5.3), in document order. `id` is an
    /// NCName; the function filters its candidates before asking.
    ///
    /// The default implementation returns no node, which is correct for a
    /// backend whose nodes are never is-idrefs (see
    /// [`is_idrefs`](Self::is_idrefs)).
    fn find_nodes_by_idref(&self, id: &str) -> Result<Vec<Self>, NavigatorError> {
        let _ = id;
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_node_type_equality() {
        assert_eq!(DomNodeType::Element, DomNodeType::Element);
        assert_ne!(DomNodeType::Element, DomNodeType::Attribute);
    }

    #[test]
    fn test_dom_node_type_is_text_like() {
        assert!(DomNodeType::Text.is_text_like());
        assert!(DomNodeType::Whitespace.is_text_like());
        assert!(DomNodeType::SignificantWhitespace.is_text_like());
        assert!(!DomNodeType::Element.is_text_like());
        assert!(!DomNodeType::Attribute.is_text_like());
        assert!(!DomNodeType::Comment.is_text_like());
    }

    #[test]
    fn test_xml_node_order() {
        assert_ne!(XmlNodeOrder::Before, XmlNodeOrder::After);
        assert_eq!(XmlNodeOrder::Same, XmlNodeOrder::Same);
    }

    #[test]
    fn test_namespace_axis_scope() {
        assert_ne!(NamespaceAxisScope::All, NamespaceAxisScope::Local);
        assert_eq!(
            NamespaceAxisScope::ExcludeXml,
            NamespaceAxisScope::ExcludeXml
        );
    }
}
