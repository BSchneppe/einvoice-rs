//! Annotations and documentation
//!
//! This module handles XSD annotations including:
//! - xs:annotation elements with xs:appinfo and xs:documentation
//! - Foreign attributes (non-XSD attributes on schema elements)
//! - XML fragment preservation for extensibility

use crate::ids::{DocumentId, NameId};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::parser::location::{SourceRef, SourceSpan};

/// XML fragment - raw XML content preserved from source
///
/// Used to capture the content of xs:appinfo and xs:documentation elements
/// without parsing it, allowing later processing by consumers.
#[derive(Debug, Clone)]
pub struct XmlFragment {
    /// Document containing this fragment
    pub doc_id: DocumentId,

    /// Byte span in the source document
    pub span: SourceSpan,
}

impl XmlFragment {
    /// Create a new XML fragment reference
    pub fn new(doc_id: DocumentId, span: SourceSpan) -> Self {
        Self { doc_id, span }
    }

    /// Get the byte range for this fragment
    pub fn byte_range(&self) -> std::ops::Range<usize> {
        self.span.start..self.span.end
    }
}

/// Foreign attribute - a non-XSD attribute on a schema element
///
/// XSD allows arbitrary attributes from non-XSD namespaces on most elements.
/// These are collected for extensibility (e.g., tool-specific annotations such as JAXB bindings).
#[derive(Debug, Clone)]
pub struct ForeignAttribute {
    /// Qualified name of the attribute
    pub namespace: Option<NameId>,
    pub local_name: NameId,
    pub prefix: Option<NameId>,

    /// Attribute value
    pub value: String,

    /// Source location
    pub source: Option<SourceRef>,
}

impl ForeignAttribute {
    /// Create a new foreign attribute
    pub fn new(namespace: Option<NameId>, local_name: NameId, value: String) -> Self {
        Self {
            namespace,
            local_name,
            prefix: None,
            value,
            source: None,
        }
    }

    /// Check if this attribute is in a given namespace
    pub fn is_in_namespace(&self, ns: Option<NameId>) -> bool {
        self.namespace == ns
    }
}

/// Annotation item - either appinfo or documentation
#[derive(Debug, Clone)]
pub enum AnnotationItem {
    /// xs:appinfo element
    AppInfo(AppInfoElement),
    /// xs:documentation element
    Documentation(DocumentationElement),
}

/// xs:appinfo element
///
/// Contains machine-readable information.
#[derive(Debug, Clone)]
pub struct AppInfoElement {
    /// Source URI attribute
    pub source: Option<String>,

    /// Foreign attributes on the appinfo element
    pub attributes: Vec<ForeignAttribute>,

    /// Namespace bindings in scope when this was parsed
    pub namespaces: NamespaceContextSnapshot,

    /// Raw XML content (not parsed)
    pub content: XmlFragment,

    /// Source location
    pub source_ref: Option<SourceRef>,
}

impl AppInfoElement {
    /// Create a new appinfo element
    pub fn new(content: XmlFragment, namespaces: NamespaceContextSnapshot) -> Self {
        Self {
            source: None,
            attributes: Vec::new(),
            namespaces,
            content,
            source_ref: None,
        }
    }
}

/// xs:documentation element
///
/// Contains human-readable documentation.
#[derive(Debug, Clone)]
pub struct DocumentationElement {
    /// Source URI attribute
    pub source: Option<String>,

    /// Language attribute (xml:lang)
    pub lang: Option<String>,

    /// Foreign attributes on the documentation element
    pub attributes: Vec<ForeignAttribute>,

    /// Namespace bindings in scope when this was parsed
    pub namespaces: NamespaceContextSnapshot,

    /// Raw XML content (not parsed)
    pub content: XmlFragment,

    /// Source location
    pub source_ref: Option<SourceRef>,
}

impl DocumentationElement {
    /// Create a new documentation element
    pub fn new(content: XmlFragment, namespaces: NamespaceContextSnapshot) -> Self {
        Self {
            source: None,
            lang: None,
            attributes: Vec::new(),
            namespaces,
            content,
            source_ref: None,
        }
    }
}

/// Annotation - contains appinfo and documentation elements
///
/// Annotations can appear on most schema elements and are used for:
/// - Human documentation
/// - Machine-readable extensions (JAXB, XBRL, etc.)
#[derive(Debug, Clone)]
pub struct Annotation {
    /// ID attribute
    pub id: Option<String>,

    /// Foreign attributes on the annotation element itself
    pub attributes: Vec<ForeignAttribute>,

    /// Annotation items (appinfo and documentation in order)
    pub items: Vec<AnnotationItem>,

    /// Source location
    pub source: Option<SourceRef>,
}

impl Annotation {
    /// Create a new empty annotation
    pub fn new() -> Self {
        Self {
            id: None,
            attributes: Vec::new(),
            items: Vec::new(),
            source: None,
        }
    }

    /// Check if this annotation is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.attributes.is_empty()
    }

    /// Add an appinfo element
    pub fn add_appinfo(&mut self, appinfo: AppInfoElement) {
        self.items.push(AnnotationItem::AppInfo(appinfo));
    }

    /// Add a documentation element
    pub fn add_documentation(&mut self, doc: DocumentationElement) {
        self.items.push(AnnotationItem::Documentation(doc));
    }

    /// Get all appinfo elements
    pub fn appinfos(&self) -> impl Iterator<Item = &AppInfoElement> {
        self.items.iter().filter_map(|item| match item {
            AnnotationItem::AppInfo(a) => Some(a),
            _ => None,
        })
    }

    /// Get all documentation elements
    pub fn documentations(&self) -> impl Iterator<Item = &DocumentationElement> {
        self.items.iter().filter_map(|item| match item {
            AnnotationItem::Documentation(d) => Some(d),
            _ => None,
        })
    }

    /// Get documentation in a specific language
    pub fn documentation_for_lang(&self, lang: &str) -> Option<&DocumentationElement> {
        self.documentations()
            .find(|d| d.lang.as_ref().is_some_and(|l| l == lang))
    }

    /// Add a foreign attribute
    pub fn add_foreign_attribute(&mut self, attr: ForeignAttribute) {
        self.attributes.push(attr);
    }
}

impl Default for Annotation {
    fn default() -> Self {
        Self::new()
    }
}

/// Implicit annotation - created from foreign attributes on schema elements
///
/// When a schema element has foreign attributes but no explicit xs:annotation,
/// an implicit annotation is created to hold them.
pub fn create_implicit_annotation(
    attrs: Vec<ForeignAttribute>,
    source: Option<SourceRef>,
) -> Annotation {
    Annotation {
        id: None,
        attributes: attrs,
        items: Vec::new(),
        source,
    }
}

/// Merge foreign attributes into an existing annotation or create an implicit one
///
/// This is used by frame finish() methods to ensure foreign attributes on schema
/// elements are preserved. If no explicit annotation exists, an implicit one is
/// created to hold the foreign attributes.
pub fn merge_foreign_attributes(
    annotation: Option<Annotation>,
    foreign_attrs: Vec<ForeignAttribute>,
    source: Option<SourceRef>,
) -> Option<Annotation> {
    if foreign_attrs.is_empty() {
        return annotation;
    }
    match annotation {
        Some(mut ann) => {
            ann.attributes.extend(foreign_attrs);
            Some(ann)
        }
        None => Some(create_implicit_annotation(foreign_attrs, source)),
    }
}

/// Helper to check if an attribute is a foreign attribute
///
/// Foreign attributes are those not in:
/// - The XSD namespace
/// - The XSI namespace
/// - No namespace (unqualified XSD attributes)
pub fn is_foreign_attribute(namespace: Option<NameId>, xsd_ns: NameId, xsi_ns: NameId) -> bool {
    match namespace {
        None => false, // Unqualified attributes are XSD attributes
        Some(ns) => ns != xsd_ns && ns != xsi_ns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_fragment() {
        let fragment = XmlFragment::new(0, SourceSpan { start: 10, end: 50 });
        assert_eq!(fragment.byte_range(), 10..50);
    }

    #[test]
    fn test_foreign_attribute() {
        let attr = ForeignAttribute::new(Some(NameId(1)), NameId(2), "value".to_string());
        assert!(attr.is_in_namespace(Some(NameId(1))));
        assert!(!attr.is_in_namespace(None));
    }

    #[test]
    fn test_annotation_empty() {
        let ann = Annotation::new();
        assert!(ann.is_empty());
    }

    #[test]
    fn test_annotation_with_items() {
        let mut ann = Annotation::new();

        let content = XmlFragment::new(0, SourceSpan { start: 0, end: 10 });
        let namespaces = NamespaceContextSnapshot {
            default_ns: None,
            bindings: vec![],
        };

        ann.add_appinfo(AppInfoElement::new(content.clone(), namespaces.clone()));
        ann.add_documentation(DocumentationElement::new(content, namespaces));

        assert!(!ann.is_empty());
        assert_eq!(ann.appinfos().count(), 1);
        assert_eq!(ann.documentations().count(), 1);
    }

    #[test]
    fn test_documentation_by_lang() {
        let mut ann = Annotation::new();

        let content = XmlFragment::new(0, SourceSpan { start: 0, end: 10 });
        let namespaces = NamespaceContextSnapshot {
            default_ns: None,
            bindings: vec![],
        };

        let mut doc_en = DocumentationElement::new(content.clone(), namespaces.clone());
        doc_en.lang = Some("en".to_string());

        let mut doc_fr = DocumentationElement::new(content, namespaces);
        doc_fr.lang = Some("fr".to_string());

        ann.add_documentation(doc_en);
        ann.add_documentation(doc_fr);

        assert!(ann.documentation_for_lang("en").is_some());
        assert!(ann.documentation_for_lang("fr").is_some());
        assert!(ann.documentation_for_lang("de").is_none());
    }

    #[test]
    fn test_implicit_annotation() {
        let attrs = vec![ForeignAttribute::new(
            Some(NameId(1)),
            NameId(2),
            "value".to_string(),
        )];

        let ann = create_implicit_annotation(attrs, None);
        assert!(!ann.is_empty());
        assert_eq!(ann.attributes.len(), 1);
    }

    #[test]
    fn test_is_foreign_attribute() {
        let xsd_ns = NameId(1);
        let xsi_ns = NameId(2);
        let other_ns = NameId(3);

        // Unqualified is not foreign
        assert!(!is_foreign_attribute(None, xsd_ns, xsi_ns));

        // XSD namespace is not foreign
        assert!(!is_foreign_attribute(Some(xsd_ns), xsd_ns, xsi_ns));

        // XSI namespace is not foreign
        assert!(!is_foreign_attribute(Some(xsi_ns), xsd_ns, xsi_ns));

        // Other namespace is foreign
        assert!(is_foreign_attribute(Some(other_ns), xsd_ns, xsi_ns));
    }
}
