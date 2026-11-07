//! Function signature definitions for XPath 2.0 functions.
//!
//! This module provides types for describing function signatures,
//! including parameter types and arity constraints.

use crate::types::sequence::{ItemType, SequenceType};

/// XPath function namespace (default for unprefixed function calls)
pub const FN_NAMESPACE: &str = "http://www.w3.org/2005/xpath-functions";

/// XPath 2010 function namespace (alias for compatibility)
pub const FN_2010_NAMESPACE: &str = "http://www.w3.org/xpath-functions";

/// Function arity specification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionArity {
    /// Fixed number of arguments
    Exact(usize),
    /// Minimum number of arguments (variadic function)
    /// For example, `concat` takes at least 2 arguments
    Variadic(usize),
    /// Range of arguments (min, max inclusive)
    Range(usize, usize),
}

impl FunctionArity {
    /// Check if a given argument count matches this arity
    pub fn matches(&self, count: usize) -> bool {
        match self {
            Self::Exact(n) => count == *n,
            Self::Variadic(min) => count >= *min,
            Self::Range(min, max) => count >= *min && count <= *max,
        }
    }

    /// Get the minimum required argument count
    pub fn min_args(&self) -> usize {
        match self {
            Self::Exact(n) => *n,
            Self::Variadic(min) => *min,
            Self::Range(min, _) => *min,
        }
    }

    /// Get the maximum allowed argument count (None for variadic)
    pub fn max_args(&self) -> Option<usize> {
        match self {
            Self::Exact(n) => Some(*n),
            Self::Variadic(_) => None,
            Self::Range(_, max) => Some(*max),
        }
    }
}

/// Function signature describing parameter and return types
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// The function namespace URI
    pub namespace: &'static str,
    /// The local name of the function
    pub local_name: &'static str,
    /// The arity specification
    pub arity: FunctionArity,
    /// Parameter types (may be shorter than actual args for variadic functions)
    pub param_types: Vec<SequenceType>,
    /// Return type
    pub return_type: SequenceType,
}

impl FunctionSignature {
    /// Create a new function signature with exact arity
    pub fn new(
        namespace: &'static str,
        local_name: &'static str,
        param_types: Vec<SequenceType>,
        return_type: SequenceType,
    ) -> Self {
        let arity = FunctionArity::Exact(param_types.len());
        Self {
            namespace,
            local_name,
            arity,
            param_types,
            return_type,
        }
    }

    /// Create a function signature with variadic arity
    pub fn variadic(
        namespace: &'static str,
        local_name: &'static str,
        min_args: usize,
        param_types: Vec<SequenceType>,
        return_type: SequenceType,
    ) -> Self {
        Self {
            namespace,
            local_name,
            arity: FunctionArity::Variadic(min_args),
            param_types,
            return_type,
        }
    }

    /// Create a function signature with range arity
    pub fn range(
        namespace: &'static str,
        local_name: &'static str,
        min_args: usize,
        max_args: usize,
        param_types: Vec<SequenceType>,
        return_type: SequenceType,
    ) -> Self {
        Self {
            namespace,
            local_name,
            arity: FunctionArity::Range(min_args, max_args),
            param_types,
            return_type,
        }
    }

    /// Check if this signature matches the given arity
    pub fn matches_arity(&self, count: usize) -> bool {
        self.arity.matches(count)
    }

    /// Get the expected type for a parameter at the given index
    ///
    /// For variadic functions, if the index exceeds the param_types length,
    /// returns the last parameter type (if any) or item()*.
    pub fn param_type(&self, index: usize) -> SequenceType {
        if index < self.param_types.len() {
            self.param_types[index].clone()
        } else if !self.param_types.is_empty() {
            // For variadic functions, use the last parameter type
            self.param_types.last().unwrap().clone()
        } else {
            // Default to any sequence
            SequenceType::any()
        }
    }
}

// Convenience constructors for common signature patterns

/// Create a signature for fn:xxx() with no arguments
pub fn sig_0(local_name: &'static str, return_type: SequenceType) -> FunctionSignature {
    FunctionSignature::new(FN_NAMESPACE, local_name, vec![], return_type)
}

/// Create a signature for fn:xxx($arg) with one argument
pub fn sig_1(
    local_name: &'static str,
    arg1: SequenceType,
    return_type: SequenceType,
) -> FunctionSignature {
    FunctionSignature::new(FN_NAMESPACE, local_name, vec![arg1], return_type)
}

/// Create a signature for fn:xxx($arg1, $arg2) with two arguments
pub fn sig_2(
    local_name: &'static str,
    arg1: SequenceType,
    arg2: SequenceType,
    return_type: SequenceType,
) -> FunctionSignature {
    FunctionSignature::new(FN_NAMESPACE, local_name, vec![arg1, arg2], return_type)
}

/// Create a signature for fn:xxx($arg1, $arg2, $arg3) with three arguments
pub fn sig_3(
    local_name: &'static str,
    arg1: SequenceType,
    arg2: SequenceType,
    arg3: SequenceType,
    return_type: SequenceType,
) -> FunctionSignature {
    FunctionSignature::new(
        FN_NAMESPACE,
        local_name,
        vec![arg1, arg2, arg3],
        return_type,
    )
}

/// Create common sequence types for function signatures
pub mod types {
    use super::*;

    /// xs:string
    pub fn string() -> SequenceType {
        SequenceType::string()
    }

    /// xs:string?
    pub fn string_opt() -> SequenceType {
        SequenceType::string_optional()
    }

    /// xs:string*
    pub fn string_star() -> SequenceType {
        SequenceType::star(ItemType::AtomicType(crate::types::XmlTypeCode::String))
    }

    /// xs:boolean
    pub fn boolean() -> SequenceType {
        SequenceType::boolean()
    }

    /// xs:integer
    pub fn integer() -> SequenceType {
        SequenceType::integer()
    }

    /// xs:integer?
    pub fn integer_opt() -> SequenceType {
        SequenceType::integer_optional()
    }

    /// xs:integer*
    pub fn integer_star() -> SequenceType {
        SequenceType::star(ItemType::AtomicType(crate::types::XmlTypeCode::Integer))
    }

    /// xs:double
    pub fn double() -> SequenceType {
        SequenceType::double()
    }

    /// xs:double?
    pub fn double_opt() -> SequenceType {
        SequenceType::double_optional()
    }

    /// xs:anyAtomicType
    pub fn any_atomic() -> SequenceType {
        SequenceType::any_atomic()
    }

    /// xs:anyAtomicType?
    pub fn any_atomic_opt() -> SequenceType {
        SequenceType::any_atomic_optional()
    }

    /// xs:anyAtomicType*
    pub fn any_atomic_star() -> SequenceType {
        SequenceType::any_atomic_star()
    }

    /// node()
    pub fn node() -> SequenceType {
        SequenceType::node()
    }

    /// node()?
    pub fn node_opt() -> SequenceType {
        SequenceType::optional(ItemType::AnyNode)
    }

    /// node()*
    pub fn nodes() -> SequenceType {
        SequenceType::nodes()
    }

    /// item()
    pub fn item() -> SequenceType {
        SequenceType::item()
    }

    /// item()?
    pub fn item_opt() -> SequenceType {
        SequenceType::optional(ItemType::AnyItem)
    }

    /// item()*
    pub fn any() -> SequenceType {
        SequenceType::any()
    }

    /// empty-sequence()
    pub fn empty() -> SequenceType {
        SequenceType::empty()
    }

    /// xs:QName
    pub fn qname() -> SequenceType {
        SequenceType::qname()
    }

    /// xs:QName?
    pub fn qname_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(crate::types::XmlTypeCode::QName))
    }

    /// xs:anyURI
    pub fn any_uri() -> SequenceType {
        SequenceType::any_uri()
    }

    /// xs:anyURI?
    pub fn any_uri_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(crate::types::XmlTypeCode::AnyUri))
    }

    /// xs:dateTime
    pub fn datetime() -> SequenceType {
        SequenceType::datetime()
    }

    /// xs:dateTime?
    pub fn datetime_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(crate::types::XmlTypeCode::DateTime))
    }

    /// xs:date
    pub fn date() -> SequenceType {
        SequenceType::date()
    }

    /// xs:date?
    pub fn date_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(crate::types::XmlTypeCode::Date))
    }

    /// xs:time
    pub fn time() -> SequenceType {
        SequenceType::time()
    }

    /// xs:time?
    pub fn time_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(crate::types::XmlTypeCode::Time))
    }

    /// xs:duration
    pub fn duration() -> SequenceType {
        SequenceType::duration()
    }

    /// xs:duration?
    pub fn duration_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(crate::types::XmlTypeCode::Duration))
    }

    /// xs:dayTimeDuration
    pub fn day_time_duration() -> SequenceType {
        SequenceType::one(ItemType::AtomicType(
            crate::types::XmlTypeCode::DayTimeDuration,
        ))
    }

    /// xs:dayTimeDuration?
    pub fn day_time_duration_opt() -> SequenceType {
        SequenceType::optional(ItemType::AtomicType(
            crate::types::XmlTypeCode::DayTimeDuration,
        ))
    }

    /// element()
    pub fn element() -> SequenceType {
        SequenceType::one(ItemType::element(None, None, false))
    }

    /// numeric (xs:double | xs:decimal | xs:float)
    pub fn numeric() -> SequenceType {
        // Use double as the general numeric type
        SequenceType::double()
    }

    /// numeric?
    pub fn numeric_opt() -> SequenceType {
        SequenceType::double_optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arity_exact() {
        let arity = FunctionArity::Exact(2);
        assert!(arity.matches(2));
        assert!(!arity.matches(1));
        assert!(!arity.matches(3));
        assert_eq!(arity.min_args(), 2);
        assert_eq!(arity.max_args(), Some(2));
    }

    #[test]
    fn test_arity_variadic() {
        let arity = FunctionArity::Variadic(2);
        assert!(!arity.matches(1));
        assert!(arity.matches(2));
        assert!(arity.matches(10));
        assert_eq!(arity.min_args(), 2);
        assert_eq!(arity.max_args(), None);
    }

    #[test]
    fn test_arity_range() {
        let arity = FunctionArity::Range(1, 3);
        assert!(!arity.matches(0));
        assert!(arity.matches(1));
        assert!(arity.matches(2));
        assert!(arity.matches(3));
        assert!(!arity.matches(4));
        assert_eq!(arity.min_args(), 1);
        assert_eq!(arity.max_args(), Some(3));
    }

    #[test]
    fn test_signature_param_type() {
        let sig = sig_2(
            "substring",
            types::string(),
            types::double(),
            types::string(),
        );

        assert_eq!(sig.param_type(0), types::string());
        assert_eq!(sig.param_type(1), types::double());
        // Out of bounds returns last type
        assert_eq!(sig.param_type(2), types::double());
    }

    #[test]
    fn test_signature_matches_arity() {
        let sig = sig_2("test", types::string(), types::string(), types::string());
        assert!(!sig.matches_arity(1));
        assert!(sig.matches_arity(2));
        assert!(!sig.matches_arity(3));
    }
}
