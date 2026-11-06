//! The well-formedness error.

use std::fmt;

/// The rule of XML 1.0 (Fifth Edition) or Namespaces in XML 1.0 (Third
/// Edition) a document breaks.
///
/// [`citation`](Self::citation) names the section and production or
/// constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WellFormednessRule {
    /// XML 1.0 §2.1 \[1\] `document ::= prolog element Misc*`: exactly one root
    /// element, and outside it only white space, comments and processing
    /// instructions — no character data, CDATA section or reference, no
    /// second element; a document without a root element.
    Document,
    /// XML 1.0 §2.2 \[2\] `Char`: a character that is not an XML character
    /// (a C0 control other than tab, line feed and carriage return; U+FFFE;
    /// U+FFFF).
    Char,
    /// XML 1.0 §2.3 \[5\] `Name`: an element, attribute or
    /// processing-instruction target name, or the name in a document type
    /// declaration, is not a Name.
    Name,
    /// XML 1.0 §2.3 \[10\] `AttValue`: a `<` in an attribute value.
    AttributeValue,
    /// XML 1.0 §2.4 \[14\] `CharData`: `]]>` in character data.
    CharData,
    /// XML 1.0 §2.5 \[15\] `Comment`: `--` inside a comment, or a comment that
    /// ends in `-`.
    Comment,
    /// XML 1.0 §2.6 \[16\] `PI`, \[17\] `PITarget`: a target `xml` in any case
    /// combination (other than the XML declaration), or a target not
    /// followed by white space.
    ProcessingInstruction,
    /// XML 1.0 §2.8 \[23\] `XMLDecl` (with \[24\]–\[26\], \[32\], \[80\], \[81\]): an XML
    /// declaration that is not at the very start of the document, has no
    /// `version`, has its pseudo-attributes out of order or unknown ones, or
    /// a `standalone` other than `yes` / `no`.
    XmlDeclaration,
    /// XML 1.0 §2.8 \[22\] `prolog`, \[28\] `doctypedecl`: a document type
    /// declaration that is not in the prolog (after the root element, inside
    /// it, or a second one), or that does not follow \[28\]: `<!DOCTYPE` (upper
    /// case) and white space, a name, an optional external identifier
    /// (\[75\] `ExternalID`, its literals \[11\]–\[13\]), an optional internal
    /// subset in brackets whose markup is balanced (comments, processing
    /// instructions, `<!ELEMENT` / `<!ATTLIST` / `<!ENTITY` / `<!NOTATION`
    /// declarations to their `>`, parameter-entity references), and `>`. The
    /// subset's declarations themselves are not checked against their own
    /// productions (they are not processed).
    DocumentTypeDeclaration,
    /// XML 1.0 §3.1 \[40\] `STag`, \[41\] `Attribute`, \[44\] `EmptyElemTag`: an
    /// attribute not preceded by white space, an attribute name not followed
    /// by `=` and a quoted value, or other characters in a tag.
    StartTag,
    /// XML 1.0 §4.1 \[66\] `CharRef`, WFC: Legal Character: a character
    /// reference that does not name an XML character, or is malformed.
    LegalCharacter,
    /// XML 1.0 §4.1 \[67\] `Reference`, \[68\] `EntityRef`: an `&` that does not
    /// start a reference `&Name;` or a character reference.
    Reference,
    /// Namespaces in XML 1.0 §4 \[7\] `QName`, §7: an element or attribute
    /// name with more than one colon, or an empty prefix or local part; a
    /// processing-instruction target with a colon.
    QName,
    /// Namespaces in XML 1.0 §3, NSC: Reserved Prefixes and Namespace Names:
    /// `xml` bound to another namespace, or another prefix (or the default
    /// namespace) bound to the XML namespace; `xmlns` declared, or the
    /// `xmlns` namespace bound; an element name with the prefix `xmlns`.
    ReservedPrefix,
    /// Namespaces in XML 1.0 §5, NSC: No Prefix Undeclaring: `xmlns:p=""`.
    PrefixUndeclaring,
    /// Namespaces in XML 1.0 §6.3, NSC: Attributes Unique: two attributes of
    /// one element with the same expanded name.
    AttributesUnique,
    /// XML 1.0 §3.1, WFC: Unique Att Spec: an attribute name twice in one
    /// tag.
    UniqueAttSpec,
    /// Namespaces in XML 1.0 §5, NSC: Prefix Declared: a prefix that no
    /// namespace declaration in scope binds (in XML 1.1 also one whose
    /// innermost declaration undeclares it).
    PrefixDeclared,
    /// XML 1.0 §4.1, WFC: Entity Declared: a reference to an entity other
    /// than the five predefined ones (the readers process no DTD).
    EntityDeclared,
    /// XML 1.0 §4.3.3: bytes that are not well-formed UTF-8 (the readers
    /// read UTF-8).
    Encoding,
    /// XML 1.0 §2.1 \[1\] `document`, \[39\] `element`: the input ends inside
    /// an element or inside markup. The message names the innermost open
    /// element as written and how many are open, e.g. "the input ends inside
    /// the element 'p:order' (2 elements not closed)".
    Truncated,
    /// XML 1.0 §3 \[39\] `element`, WFC: Element Type Match: an end tag that
    /// does not match the open element, or one with none open.
    ElementTypeMatch,
    /// XML 1.0 §2.4: markup of no kind the grammar has (a `<!` that starts
    /// no comment, CDATA section or document type declaration).
    Markup,
    /// XML 1.1 §2.11: a NEL (U+0085) or LINE SEPARATOR (U+2028) used as
    /// white space inside a tag or a declaration. XML 1.1 reads them as line
    /// feeds; this reader does not support them inside markup (they are
    /// accepted, and normalized, in character data and attribute values) —
    /// and inside the XML declaration they are a fatal error
    /// ([`XmlDeclaration`](Self::XmlDeclaration)).
    Xml11LineEndInMarkup,
}

impl WellFormednessRule {
    /// The specification, section and production or constraint, e.g.
    /// `"XML 1.0 §2.5 \[15\] Comment"`.
    pub fn citation(self) -> &'static str {
        match self {
            WellFormednessRule::Document => "XML 1.0 §2.1 [1] document",
            WellFormednessRule::Char => "XML 1.0 §2.2 [2] Char",
            WellFormednessRule::Name => "XML 1.0 §2.3 [5] Name",
            WellFormednessRule::AttributeValue => "XML 1.0 §2.3 [10] AttValue",
            WellFormednessRule::CharData => "XML 1.0 §2.4 [14] CharData",
            WellFormednessRule::Comment => "XML 1.0 §2.5 [15] Comment",
            WellFormednessRule::ProcessingInstruction => "XML 1.0 §2.6 [16] PI, [17] PITarget",
            WellFormednessRule::XmlDeclaration => "XML 1.0 §2.8 [23] XMLDecl",
            WellFormednessRule::DocumentTypeDeclaration => {
                "XML 1.0 §2.8 [22] prolog, [28] doctypedecl"
            }
            WellFormednessRule::StartTag => {
                "XML 1.0 §3.1 [40] STag, [41] Attribute, [44] EmptyElemTag"
            }
            WellFormednessRule::LegalCharacter => "XML 1.0 §4.1 [66] CharRef, WFC: Legal Character",
            WellFormednessRule::Reference => "XML 1.0 §4.1 [67] Reference",
            WellFormednessRule::QName => "Namespaces in XML 1.0 §4 [7] QName",
            WellFormednessRule::ReservedPrefix => {
                "Namespaces in XML 1.0 §3, NSC: Reserved Prefixes and Namespace Names"
            }
            WellFormednessRule::PrefixUndeclaring => {
                "Namespaces in XML 1.0 §5, NSC: No Prefix Undeclaring"
            }
            WellFormednessRule::AttributesUnique => {
                "Namespaces in XML 1.0 §6.3, NSC: Attributes Unique"
            }
            WellFormednessRule::UniqueAttSpec => "XML 1.0 §3.1, WFC: Unique Att Spec",
            WellFormednessRule::PrefixDeclared => "Namespaces in XML 1.0 §5, NSC: Prefix Declared",
            WellFormednessRule::EntityDeclared => "XML 1.0 §4.1, WFC: Entity Declared",
            WellFormednessRule::Encoding => "XML 1.0 §4.3.3 Character Encoding in Entities",
            WellFormednessRule::Truncated => "XML 1.0 §2.1 [1] document, [39] element",
            WellFormednessRule::ElementTypeMatch => {
                "XML 1.0 §3 [39] element, WFC: Element Type Match"
            }
            WellFormednessRule::Markup => "XML 1.0 §2.4 Character Data and Markup",
            WellFormednessRule::Xml11LineEndInMarkup => {
                "XML 1.1 §2.11 End-of-Line Handling (not supported inside markup)"
            }
        }
    }

    /// The citation for a document read with the rules of `version`: the
    /// XML 1.1 and Namespaces in XML 1.1 sections when 1.1 applied.
    pub fn citation_in(self, version: XmlVersion) -> &'static str {
        if version == XmlVersion::V1_0 {
            return self.citation();
        }
        match self {
            WellFormednessRule::Document => "XML 1.1 §2.1 [1] document",
            WellFormednessRule::Char => "XML 1.1 §2.2 [2] Char, [2a] RestrictedChar",
            WellFormednessRule::Name => "XML 1.1 §2.3 [5] Name",
            WellFormednessRule::AttributeValue => "XML 1.1 §2.3 [10] AttValue",
            WellFormednessRule::CharData => "XML 1.1 §2.4 [14] CharData",
            WellFormednessRule::Comment => "XML 1.1 §2.5 [15] Comment",
            WellFormednessRule::ProcessingInstruction => "XML 1.1 §2.6 [16] PI, [17] PITarget",
            WellFormednessRule::XmlDeclaration => "XML 1.1 §2.8 [23] XMLDecl",
            WellFormednessRule::DocumentTypeDeclaration => {
                "XML 1.1 §2.8 [22] prolog, [28] doctypedecl"
            }
            WellFormednessRule::StartTag => {
                "XML 1.1 §3.1 [40] STag, [41] Attribute, [44] EmptyElemTag"
            }
            WellFormednessRule::LegalCharacter => "XML 1.1 §4.1 [66] CharRef, WFC: Legal Character",
            WellFormednessRule::Reference => "XML 1.1 §4.1 [67] Reference",
            WellFormednessRule::QName => "Namespaces in XML 1.1 §4 [7] QName",
            WellFormednessRule::ReservedPrefix | WellFormednessRule::PrefixUndeclaring => {
                "Namespaces in XML 1.1 §3, NSC: Reserved Prefixes and Namespace Names"
            }
            WellFormednessRule::AttributesUnique => {
                "Namespaces in XML 1.1 §6.3 Uniqueness of Attributes"
            }
            WellFormednessRule::UniqueAttSpec => "XML 1.1 §3.1, WFC: Unique Att Spec",
            WellFormednessRule::PrefixDeclared => "Namespaces in XML 1.1 §5, NSC: Prefix Declared",
            WellFormednessRule::EntityDeclared => "XML 1.1 §4.1, WFC: Entity Declared",
            WellFormednessRule::Encoding => "XML 1.1 §4.3.3 Character Encoding in Entities",
            WellFormednessRule::Truncated => "XML 1.1 §2.1 [1] document, [39] element",
            WellFormednessRule::ElementTypeMatch => {
                "XML 1.1 §3 [39] element, WFC: Element Type Match"
            }
            WellFormednessRule::Markup => "XML 1.1 §2.4 Character Data and Markup",
            WellFormednessRule::Xml11LineEndInMarkup => self.citation(),
        }
    }
}

/// The version of XML whose rules a document is read with.
///
/// A document declared `version="1.1"` is read with XML 1.1 (Second
/// Edition) and Namespaces in XML 1.1 (Second Edition); every other document
/// — no declaration (XML 1.1 §4.3.4: "it is treated as if labeled as version
/// 1.0"), `1.0`, or another `1.x` (XML 1.0 §2.8: "When an XML 1.0 processor
/// encounters a document that specifies a 1.x version number other than
/// '1.0', it will process it as a 1.0 document") — with XML 1.0 (Fifth
/// Edition) and Namespaces in XML 1.0 (Third Edition).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum XmlVersion {
    /// XML 1.0.
    #[default]
    V1_0,
    /// XML 1.1.
    V1_1,
}

/// The XML declaration of a document (XML 1.0 §2.8 \[23\] `XMLDecl`), as
/// written.
///
/// Reported to `ValidationEventHandler::on_xml_declaration` and kept by a
/// `BufferDocument` read from text. The readers read UTF-8 whatever
/// `encoding` names (a host that decodes the bytes first, e.g. with
/// `decode_xml_bytes`, keeps the original declaration in the text): a host
/// that must refuse an encoding it did not decode does so there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct XmlDeclaration {
    /// The `version` pseudo-attribute, e.g. `"1.0"`.
    pub version: String,
    /// The `encoding` pseudo-attribute, if any, as written.
    pub encoding: Option<String>,
    /// The `standalone` pseudo-attribute, if any.
    pub standalone: Option<bool>,
}

impl XmlDeclaration {
    /// The version whose rules the document is read with: XML 1.1 for
    /// exactly `1.1`, XML 1.0 otherwise.
    pub fn xml_version(&self) -> XmlVersion {
        if self.version == "1.1" {
            XmlVersion::V1_1
        } else {
            XmlVersion::V1_0
        }
    }
}

impl fmt::Display for WellFormednessRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.citation())
    }
}

/// A document is not well-formed (XML 1.0 §1.2: a fatal error) or not
/// namespace-well-formed.
///
/// `Display`: `not well-formed (XML 1.0 §2.5 \[15\] Comment) at byte 17: '--'
/// inside a comment`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotWellFormed {
    rule: WellFormednessRule,
    position: u64,
    message: Box<str>,
    version: XmlVersion,
}

impl NotWellFormed {
    pub(crate) fn new(
        rule: WellFormednessRule,
        position: u64,
        message: impl Into<Box<str>>,
        version: XmlVersion,
    ) -> Self {
        NotWellFormed {
            rule,
            position,
            message: message.into(),
            version,
        }
    }

    /// The version of XML whose rules were applied (`Display` cites that
    /// version's production).
    pub fn xml_version(&self) -> XmlVersion {
        self.version
    }

    /// The rule broken.
    pub fn rule(&self) -> WellFormednessRule {
        self.rule
    }

    /// The byte offset in the input (a leading byte-order mark included)
    /// where the violation is: the offending character, name or markup.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// What was found, without the rule and the position.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for NotWellFormed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not well-formed ({}) at byte {}: {}",
            self.rule.citation_in(self.version),
            self.position,
            self.message
        )
    }
}

impl std::error::Error for NotWellFormed {}
