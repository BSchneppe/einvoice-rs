//! Quantified expression support for XPath evaluation.
//!
//! This module implements XPath 2.0 quantified expressions:
//! - `some $x in ... satisfies ...`
//! - `every $x in ... satisfies ...`

use super::boolean::effective_boolean_value;
use super::error::XPathError;
use crate::types::value::XmlValue;

/// Check if some item in the sequence satisfies the condition.
///
/// This implements `some $x in $seq satisfies $condition`.
/// Returns true if at least one item evaluates to true.
///
/// # Arguments
///
/// * `values` - The sequence of boolean values (or values convertible to boolean)
///
/// # Returns
///
/// `true` if any value is true, `false` if all are false or sequence is empty.
pub fn some(values: &[XmlValue]) -> Result<bool, XPathError> {
    for value in values {
        if effective_boolean_value(value)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check if every item in the sequence satisfies the condition.
///
/// This implements `every $x in $seq satisfies $condition`.
/// Returns true if all items evaluate to true (including empty sequence).
///
/// # Arguments
///
/// * `values` - The sequence of boolean values (or values convertible to boolean)
///
/// # Returns
///
/// `true` if all values are true or sequence is empty, `false` otherwise.
pub fn every(values: &[XmlValue]) -> Result<bool, XPathError> {
    for value in values {
        if !effective_boolean_value(value)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Check if some value in the sequence matches a predicate.
///
/// Generic version that takes a predicate function.
///
/// # Arguments
///
/// * `values` - The sequence of values
/// * `predicate` - Function to test each value
///
/// # Returns
///
/// `true` if any value satisfies the predicate.
pub fn some_with<F>(values: &[XmlValue], predicate: F) -> bool
where
    F: FnMut(&XmlValue) -> bool,
{
    values.iter().any(predicate)
}

/// Check if every value in the sequence matches a predicate.
///
/// Generic version that takes a predicate function.
///
/// # Arguments
///
/// * `values` - The sequence of values
/// * `predicate` - Function to test each value
///
/// # Returns
///
/// `true` if all values satisfy the predicate.
pub fn every_with<F>(values: &[XmlValue], predicate: F) -> bool
where
    F: FnMut(&XmlValue) -> bool,
{
    values.iter().all(predicate)
}

/// Check if the sequence contains at least one true value.
///
/// Simplified version for boolean sequences.
pub fn some_true(values: &[bool]) -> bool {
    values.iter().any(|&v| v)
}

/// Check if all values in the sequence are true.
///
/// Simplified version for boolean sequences.
pub fn every_true(values: &[bool]) -> bool {
    values.iter().all(|&v| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_some_with_true_values() {
        let values = vec![
            XmlValue::boolean(false),
            XmlValue::boolean(true),
            XmlValue::boolean(false),
        ];
        assert!(some(&values).unwrap());
    }

    #[test]
    fn test_some_with_all_false() {
        let values = vec![XmlValue::boolean(false), XmlValue::boolean(false)];
        assert!(!some(&values).unwrap());
    }

    #[test]
    fn test_some_empty_sequence() {
        let values: Vec<XmlValue> = vec![];
        assert!(!some(&values).unwrap());
    }

    #[test]
    fn test_every_with_all_true() {
        let values = vec![XmlValue::boolean(true), XmlValue::boolean(true)];
        assert!(every(&values).unwrap());
    }

    #[test]
    fn test_every_with_some_false() {
        let values = vec![
            XmlValue::boolean(true),
            XmlValue::boolean(false),
            XmlValue::boolean(true),
        ];
        assert!(!every(&values).unwrap());
    }

    #[test]
    fn test_every_empty_sequence() {
        // Vacuous truth: every item in empty sequence satisfies any condition
        let values: Vec<XmlValue> = vec![];
        assert!(every(&values).unwrap());
    }

    #[test]
    fn test_some_with_strings() {
        // Non-empty strings are true
        let values = vec![XmlValue::string(""), XmlValue::string("hello")];
        assert!(some(&values).unwrap());
    }

    #[test]
    fn test_every_with_strings() {
        // All non-empty strings
        let values = vec![XmlValue::string("a"), XmlValue::string("b")];
        assert!(every(&values).unwrap());

        // One empty string
        let values = vec![XmlValue::string("a"), XmlValue::string("")];
        assert!(!every(&values).unwrap());
    }

    #[test]
    fn test_some_with_predicate() {
        let values = vec![
            XmlValue::string("apple"),
            XmlValue::string("banana"),
            XmlValue::string("cherry"),
        ];
        assert!(some_with(&values, |v| v.to_string_value().starts_with('b')));
        assert!(!some_with(&values, |v| v
            .to_string_value()
            .starts_with('z')));
    }

    #[test]
    fn test_every_with_predicate() {
        let values = vec![
            XmlValue::string("apple"),
            XmlValue::string("avocado"),
            XmlValue::string("apricot"),
        ];
        assert!(every_with(&values, |v| v
            .to_string_value()
            .starts_with('a')));
        assert!(!every_with(&values, |v| v.to_string_value().len() == 5));
    }

    #[test]
    fn test_some_true_bool_slice() {
        assert!(some_true(&[false, true, false]));
        assert!(!some_true(&[false, false]));
        assert!(!some_true(&[]));
    }

    #[test]
    fn test_every_true_bool_slice() {
        assert!(every_true(&[true, true, true]));
        assert!(!every_true(&[true, false, true]));
        assert!(every_true(&[])); // Vacuous truth
    }
}
