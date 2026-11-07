//! General-reference resolution for the `quick-xml` event stream.
//!
//! Since quick-xml 0.38 the reader no longer expands `&...;` inside character
//! data: a run of text containing references is reported as alternating
//! [`Event::Text`] and [`Event::GeneralRef`] events. This module resolves a
//! single [`BytesRef`] the way an XML processor without a DTD must: character
//! references (`&#NN;` / `&#xHH;`) are expanded, the five predefined entities
//! (`amp`, `lt`, `gt`, `apos`, `quot`) are substituted, and anything else is
//! an unrecognized entity.
//!
//! [`Event::Text`]: quick_xml::events::Event::Text
//! [`Event::GeneralRef`]: quick_xml::events::Event::GeneralRef

use std::borrow::Cow;

use quick_xml::escape::{resolve_predefined_entity, EscapeError};
use quick_xml::events::BytesRef;

/// Resolve a general reference into its replacement text.
///
/// Returns [`quick_xml::Error::Escape`] with [`EscapeError::UnrecognizedEntity`]
/// for entities that are neither character references nor one of the five
/// predefined XML entities — matching what `unescape()` reported before the
/// reference events were split out of text.
pub(crate) fn resolve_general_ref(r: &BytesRef<'_>) -> Result<Cow<'static, str>, quick_xml::Error> {
    if let Some(ch) = r.resolve_char_ref()? {
        return Ok(Cow::Owned(ch.to_string()));
    }

    let name = r.decode()?;
    match resolve_predefined_entity(&name) {
        Some(replacement) => Ok(Cow::Borrowed(replacement)),
        None => Err(EscapeError::UnrecognizedEntity(0..name.len(), name.into_owned()).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_predefined_entities() {
        for (name, expected) in [
            ("amp", "&"),
            ("lt", "<"),
            ("gt", ">"),
            ("apos", "'"),
            ("quot", "\""),
        ] {
            let r = BytesRef::new(name);
            assert_eq!(resolve_general_ref(&r).unwrap(), expected);
        }
    }

    #[test]
    fn resolves_character_references() {
        assert_eq!(resolve_general_ref(&BytesRef::new("#65")).unwrap(), "A");
        assert_eq!(resolve_general_ref(&BytesRef::new("#x41")).unwrap(), "A");
        assert_eq!(resolve_general_ref(&BytesRef::new("#x20")).unwrap(), " ");
    }

    #[test]
    fn rejects_unknown_entity() {
        let err = resolve_general_ref(&BytesRef::new("nope")).unwrap_err();
        assert!(matches!(
            err,
            quick_xml::Error::Escape(EscapeError::UnrecognizedEntity(_, _))
        ));
    }
}
