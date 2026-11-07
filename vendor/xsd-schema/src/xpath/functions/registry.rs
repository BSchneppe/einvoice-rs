//! XPath function registry.
//!
//! This module provides the `FunctionRegistry` for looking up function
//! definitions by namespace, local name, and arity.

use std::collections::HashMap;

use once_cell::sync::Lazy;

use super::signature::{FunctionArity, FunctionSignature, FN_2010_NAMESPACE, FN_NAMESPACE};
use super::FunctionId;
use crate::types::sequence::SequenceType;

/// Key for function lookup in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionKey {
    /// Namespace URI
    pub namespace: String,
    /// Local name
    pub local_name: String,
    /// Number of arguments
    pub arity: usize,
}

impl FunctionKey {
    /// Create a new function key
    pub fn new(namespace: impl Into<String>, local_name: impl Into<String>, arity: usize) -> Self {
        Self {
            namespace: namespace.into(),
            local_name: local_name.into(),
            arity,
        }
    }
}

/// Entry in the function registry combining ID and signature.
#[derive(Debug, Clone)]
pub struct FunctionEntry {
    /// The function identifier for dispatch
    pub id: FunctionId,
    /// The function signature with type information
    pub signature: FunctionSignature,
}

impl FunctionEntry {
    /// Create a new function entry
    pub fn new(id: FunctionId, signature: FunctionSignature) -> Self {
        Self { id, signature }
    }
}

/// Registry of all built-in XPath functions.
///
/// Provides lookup by namespace, local name, and arity.
pub struct FunctionRegistry {
    /// All registered function entries
    entries: Vec<FunctionEntry>,
    /// Lookup map from (namespace, local_name, arity) to entry index
    lookup: HashMap<FunctionKey, usize>,
    /// Lookup map for variadic functions: (namespace, local_name) -> entry index
    /// Used when exact arity lookup fails
    variadic_lookup: HashMap<(String, String), usize>,
}

impl FunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            lookup: HashMap::new(),
            variadic_lookup: HashMap::new(),
        }
    }

    /// Register a function entry
    pub fn register(&mut self, entry: FunctionEntry) {
        let index = self.entries.len();
        let sig = &entry.signature;

        // Register for each valid arity
        match sig.arity {
            FunctionArity::Exact(n) => {
                let key =
                    FunctionKey::new(sig.namespace.to_string(), sig.local_name.to_string(), n);
                self.lookup.insert(key, index);
            }
            FunctionArity::Range(min, max) => {
                for arity in min..=max {
                    let key = FunctionKey::new(
                        sig.namespace.to_string(),
                        sig.local_name.to_string(),
                        arity,
                    );
                    self.lookup.insert(key, index);
                }
            }
            FunctionArity::Variadic(_) => {
                // For variadic, register in the variadic lookup
                self.variadic_lookup.insert(
                    (sig.namespace.to_string(), sig.local_name.to_string()),
                    index,
                );
            }
        }

        self.entries.push(entry);
    }

    /// Look up a function by namespace, local name, and arity.
    ///
    /// Also handles the XPath 2010 namespace alias.
    pub fn lookup(
        &self,
        namespace: &str,
        local_name: &str,
        arity: usize,
    ) -> Option<&FunctionEntry> {
        // Try exact lookup first
        let key = FunctionKey {
            namespace: namespace.to_string(),
            local_name: local_name.to_string(),
            arity,
        };
        if let Some(&index) = self.lookup.get(&key) {
            return Some(&self.entries[index]);
        }

        // Try variadic lookup
        let variadic_key = (namespace.to_string(), local_name.to_string());
        if let Some(&index) = self.variadic_lookup.get(&variadic_key) {
            let entry = &self.entries[index];
            if entry.signature.arity.matches(arity) {
                return Some(entry);
            }
        }

        // If namespace is the 2010 function namespace, try the standard namespace
        if namespace == FN_2010_NAMESPACE {
            return self.lookup(FN_NAMESPACE, local_name, arity);
        }

        None
    }

    /// Get an entry by its FunctionId.
    pub fn by_id(&self, id: FunctionId) -> Option<&FunctionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Get an entry by its FunctionId discriminant value.
    ///
    /// This is used internally for converting FunctionHandle back to FunctionId.
    pub fn by_id_value(&self, value: u16) -> Option<&FunctionEntry> {
        self.entries.iter().find(|e| (e.id as u16) == value)
    }

    /// Get a slice of all registered entries.
    pub fn entries(&self) -> &[FunctionEntry] {
        &self.entries
    }

    /// Get the number of registered functions
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Static Function Registry
// ============================================================================

/// The global function registry containing all built-in XPath 2.0 functions.
pub static FUNCTION_REGISTRY: Lazy<FunctionRegistry> = Lazy::new(|| {
    let mut registry = FunctionRegistry::new();
    register_all_functions(&mut registry);
    registry
});

/// Register all built-in functions in the registry.
fn register_all_functions(registry: &mut FunctionRegistry) {
    // Import type helpers
    use super::signature::types::*;

    // ========================================================================
    // Boolean Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::True,
        FunctionSignature::new(FN_NAMESPACE, "true", vec![], boolean()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::False,
        FunctionSignature::new(FN_NAMESPACE, "false", vec![], boolean()),
    ));
    // F&O §9.3.1 / §15.1.1: `fn:not($arg as item()*)`, `fn:boolean($arg as
    // item()*)`. The declared `item()*` matters: XPath 1.0 compatibility mode
    // replaces an argument whose expected type is a single item by its first
    // item (XPath 2.0 §3.1.5), which would make `boolean((1, 2))` true instead
    // of FORG0006.
    registry.register(FunctionEntry::new(
        FunctionId::Not,
        FunctionSignature::new(FN_NAMESPACE, "not", vec![any()], boolean()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Boolean,
        FunctionSignature::new(FN_NAMESPACE, "boolean", vec![any()], boolean()),
    ));

    // ========================================================================
    // Context Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Position,
        FunctionSignature::new(FN_NAMESPACE, "position", vec![], integer()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Last,
        FunctionSignature::new(FN_NAMESPACE, "last", vec![], integer()),
    ));

    // ========================================================================
    // Sequence Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Empty,
        FunctionSignature::new(FN_NAMESPACE, "empty", vec![any()], boolean()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Exists,
        FunctionSignature::new(FN_NAMESPACE, "exists", vec![any()], boolean()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Count,
        FunctionSignature::new(FN_NAMESPACE, "count", vec![any()], integer()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Reverse,
        FunctionSignature::new(FN_NAMESPACE, "reverse", vec![any()], any()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::ZeroOrOne,
        FunctionSignature::new(FN_NAMESPACE, "zero-or-one", vec![any()], item_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::OneOrMore,
        FunctionSignature::new(FN_NAMESPACE, "one-or-more", vec![any()], any()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::ExactlyOne,
        FunctionSignature::new(FN_NAMESPACE, "exactly-one", vec![any()], item()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DistinctValues,
        FunctionSignature::range(
            FN_NAMESPACE,
            "distinct-values",
            1,
            2,
            vec![any_atomic_star(), string()],
            any_atomic_star(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::IndexOf,
        FunctionSignature::range(
            FN_NAMESPACE,
            "index-of",
            2,
            3,
            vec![any_atomic_star(), any_atomic(), string()],
            integer_star(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Remove,
        FunctionSignature::new(FN_NAMESPACE, "remove", vec![any(), integer()], any()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::InsertBefore,
        FunctionSignature::new(
            FN_NAMESPACE,
            "insert-before",
            vec![any(), integer(), any()],
            any(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Subsequence,
        FunctionSignature::range(
            FN_NAMESPACE,
            "subsequence",
            2,
            3,
            vec![any(), double(), double()],
            any(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Unordered,
        FunctionSignature::new(FN_NAMESPACE, "unordered", vec![any()], any()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DeepEqual,
        FunctionSignature::range(
            FN_NAMESPACE,
            "deep-equal",
            2,
            3,
            vec![any(), any(), string()],
            boolean(),
        ),
    ));

    // ========================================================================
    // Aggregate Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Sum,
        FunctionSignature::range(
            FN_NAMESPACE,
            "sum",
            1,
            2,
            vec![any_atomic_star(), any_atomic_opt()],
            any_atomic(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Avg,
        FunctionSignature::new(
            FN_NAMESPACE,
            "avg",
            vec![any_atomic_star()],
            any_atomic_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Min,
        FunctionSignature::range(
            FN_NAMESPACE,
            "min",
            1,
            2,
            vec![any_atomic_star(), string()],
            any_atomic_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Max,
        FunctionSignature::range(
            FN_NAMESPACE,
            "max",
            1,
            2,
            vec![any_atomic_star(), string()],
            any_atomic_opt(),
        ),
    ));

    // ========================================================================
    // String Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Concat,
        FunctionSignature::variadic(
            FN_NAMESPACE,
            "concat",
            2,
            vec![any_atomic_opt(), any_atomic_opt()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::StringJoin,
        FunctionSignature::new(
            FN_NAMESPACE,
            "string-join",
            vec![string_star(), string()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Substring,
        FunctionSignature::range(
            FN_NAMESPACE,
            "substring",
            2,
            3,
            vec![string_opt(), double(), double()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::StringLength,
        FunctionSignature::range(
            FN_NAMESPACE,
            "string-length",
            0,
            1,
            vec![string_opt()],
            integer(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::NormalizeSpace,
        FunctionSignature::range(
            FN_NAMESPACE,
            "normalize-space",
            0,
            1,
            vec![string_opt()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::NormalizeUnicode,
        FunctionSignature::range(
            FN_NAMESPACE,
            "normalize-unicode",
            1,
            2,
            vec![string_opt(), string()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::UpperCase,
        FunctionSignature::new(FN_NAMESPACE, "upper-case", vec![string_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::LowerCase,
        FunctionSignature::new(FN_NAMESPACE, "lower-case", vec![string_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Translate,
        FunctionSignature::new(
            FN_NAMESPACE,
            "translate",
            vec![string_opt(), string(), string()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::EncodeForUri,
        FunctionSignature::new(FN_NAMESPACE, "encode-for-uri", vec![string_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::IriToUri,
        FunctionSignature::new(FN_NAMESPACE, "iri-to-uri", vec![string_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::EscapeHtmlUri,
        FunctionSignature::new(
            FN_NAMESPACE,
            "escape-html-uri",
            vec![string_opt()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Contains,
        FunctionSignature::range(
            FN_NAMESPACE,
            "contains",
            2,
            3,
            vec![string_opt(), string_opt(), string()],
            boolean(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::StartsWith,
        FunctionSignature::range(
            FN_NAMESPACE,
            "starts-with",
            2,
            3,
            vec![string_opt(), string_opt(), string()],
            boolean(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::EndsWith,
        FunctionSignature::range(
            FN_NAMESPACE,
            "ends-with",
            2,
            3,
            vec![string_opt(), string_opt(), string()],
            boolean(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::SubstringBefore,
        FunctionSignature::range(
            FN_NAMESPACE,
            "substring-before",
            2,
            3,
            vec![string_opt(), string_opt(), string()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::SubstringAfter,
        FunctionSignature::range(
            FN_NAMESPACE,
            "substring-after",
            2,
            3,
            vec![string_opt(), string_opt(), string()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::StringToCodepoints,
        FunctionSignature::new(
            FN_NAMESPACE,
            "string-to-codepoints",
            vec![string_opt()],
            integer_star(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::CodepointsToString,
        FunctionSignature::new(
            FN_NAMESPACE,
            "codepoints-to-string",
            vec![integer_star()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Compare,
        FunctionSignature::range(
            FN_NAMESPACE,
            "compare",
            2,
            3,
            vec![string_opt(), string_opt(), string()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::CodepointEqual,
        FunctionSignature::new(
            FN_NAMESPACE,
            "codepoint-equal",
            vec![string_opt(), string_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::Boolean,
            )),
        ),
    ));

    // ========================================================================
    // Numeric Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Abs,
        FunctionSignature::new(FN_NAMESPACE, "abs", vec![numeric_opt()], numeric_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Ceiling,
        FunctionSignature::new(FN_NAMESPACE, "ceiling", vec![numeric_opt()], numeric_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Floor,
        FunctionSignature::new(FN_NAMESPACE, "floor", vec![numeric_opt()], numeric_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Round,
        FunctionSignature::new(FN_NAMESPACE, "round", vec![numeric_opt()], numeric_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::RoundHalfToEven,
        FunctionSignature::range(
            FN_NAMESPACE,
            "round-half-to-even",
            1,
            2,
            vec![numeric_opt(), integer()],
            numeric_opt(),
        ),
    ));

    // ========================================================================
    // Node Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Name,
        FunctionSignature::range(FN_NAMESPACE, "name", 0, 1, vec![node_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::LocalName,
        FunctionSignature::range(FN_NAMESPACE, "local-name", 0, 1, vec![node_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::NamespaceUri,
        FunctionSignature::range(
            FN_NAMESPACE,
            "namespace-uri",
            0,
            1,
            vec![node_opt()],
            any_uri(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::NodeName,
        FunctionSignature::new(FN_NAMESPACE, "node-name", vec![node_opt()], qname_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Nilled,
        FunctionSignature::new(
            FN_NAMESPACE,
            "nilled",
            vec![node_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::Boolean,
            )),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::BaseUri,
        FunctionSignature::range(
            FN_NAMESPACE,
            "base-uri",
            0,
            1,
            vec![node_opt()],
            any_uri_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DocumentUri,
        FunctionSignature::new(
            FN_NAMESPACE,
            "document-uri",
            vec![node_opt()],
            any_uri_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Lang,
        FunctionSignature::range(
            FN_NAMESPACE,
            "lang",
            1,
            2,
            vec![string_opt(), node()],
            boolean(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Root,
        FunctionSignature::range(FN_NAMESPACE, "root", 0, 1, vec![node_opt()], node_opt()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Id,
        FunctionSignature::range(
            FN_NAMESPACE,
            "id",
            1,
            2,
            vec![string_star(), node()],
            SequenceType::star(crate::types::sequence::ItemType::AnyNode),
        ),
    ));
    // fn:idref($arg as xs:string*) / fn:idref($arg as xs:string*, $node as
    // node()) as node()* (F&O 1.0 §15.5.3).
    registry.register(FunctionEntry::new(
        FunctionId::Idref,
        FunctionSignature::range(
            FN_NAMESPACE,
            "idref",
            1,
            2,
            vec![string_star(), node()],
            SequenceType::star(crate::types::sequence::ItemType::AnyNode),
        ),
    ));
    // fn:collection() / fn:collection($arg) — without an external default
    // collection or a registered URI, both forms return the empty sequence
    // (XPath 2.0 functions §15.5.6). This minimal stub is sufficient for
    // CTA tests that probe `empty(collection())`.
    registry.register(FunctionEntry::new(
        FunctionId::Collection,
        FunctionSignature::range(
            FN_NAMESPACE,
            "collection",
            0,
            1,
            vec![string_opt()],
            SequenceType::star(crate::types::sequence::ItemType::AnyNode),
        ),
    ));

    // ========================================================================
    // DateTime Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::DateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "dateTime",
            vec![date_opt(), time_opt()],
            datetime_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::CurrentDateTime,
        FunctionSignature::new(FN_NAMESPACE, "current-dateTime", vec![], datetime()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::CurrentDate,
        FunctionSignature::new(FN_NAMESPACE, "current-date", vec![], date()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::CurrentTime,
        FunctionSignature::new(FN_NAMESPACE, "current-time", vec![], time()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::ImplicitTimezone,
        FunctionSignature::new(
            FN_NAMESPACE,
            "implicit-timezone",
            vec![],
            day_time_duration(),
        ),
    ));

    // Duration component extraction
    registry.register(FunctionEntry::new(
        FunctionId::YearsFromDuration,
        FunctionSignature::new(
            FN_NAMESPACE,
            "years-from-duration",
            vec![duration_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::MonthsFromDuration,
        FunctionSignature::new(
            FN_NAMESPACE,
            "months-from-duration",
            vec![duration_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DaysFromDuration,
        FunctionSignature::new(
            FN_NAMESPACE,
            "days-from-duration",
            vec![duration_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::HoursFromDuration,
        FunctionSignature::new(
            FN_NAMESPACE,
            "hours-from-duration",
            vec![duration_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::MinutesFromDuration,
        FunctionSignature::new(
            FN_NAMESPACE,
            "minutes-from-duration",
            vec![duration_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::SecondsFromDuration,
        FunctionSignature::new(
            FN_NAMESPACE,
            "seconds-from-duration",
            vec![duration_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::Decimal,
            )),
        ),
    ));

    // DateTime component extraction
    registry.register(FunctionEntry::new(
        FunctionId::YearFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "year-from-dateTime",
            vec![datetime_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::MonthFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "month-from-dateTime",
            vec![datetime_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DayFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "day-from-dateTime",
            vec![datetime_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::HoursFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "hours-from-dateTime",
            vec![datetime_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::MinutesFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "minutes-from-dateTime",
            vec![datetime_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::SecondsFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "seconds-from-dateTime",
            vec![datetime_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::Decimal,
            )),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::TimezoneFromDateTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "timezone-from-dateTime",
            vec![datetime_opt()],
            day_time_duration_opt(),
        ),
    ));

    // Date component extraction
    registry.register(FunctionEntry::new(
        FunctionId::YearFromDate,
        FunctionSignature::new(
            FN_NAMESPACE,
            "year-from-date",
            vec![date_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::MonthFromDate,
        FunctionSignature::new(
            FN_NAMESPACE,
            "month-from-date",
            vec![date_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DayFromDate,
        FunctionSignature::new(
            FN_NAMESPACE,
            "day-from-date",
            vec![date_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::TimezoneFromDate,
        FunctionSignature::new(
            FN_NAMESPACE,
            "timezone-from-date",
            vec![date_opt()],
            day_time_duration_opt(),
        ),
    ));

    // Time component extraction
    registry.register(FunctionEntry::new(
        FunctionId::HoursFromTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "hours-from-time",
            vec![time_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::MinutesFromTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "minutes-from-time",
            vec![time_opt()],
            integer_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::SecondsFromTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "seconds-from-time",
            vec![time_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::Decimal,
            )),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::TimezoneFromTime,
        FunctionSignature::new(
            FN_NAMESPACE,
            "timezone-from-time",
            vec![time_opt()],
            day_time_duration_opt(),
        ),
    ));

    // Timezone adjustment
    registry.register(FunctionEntry::new(
        FunctionId::AdjustDateTimeToTimezone,
        FunctionSignature::range(
            FN_NAMESPACE,
            "adjust-dateTime-to-timezone",
            1,
            2,
            vec![datetime_opt(), day_time_duration_opt()],
            datetime_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::AdjustDateToTimezone,
        FunctionSignature::range(
            FN_NAMESPACE,
            "adjust-date-to-timezone",
            1,
            2,
            vec![date_opt(), day_time_duration_opt()],
            date_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::AdjustTimeToTimezone,
        FunctionSignature::range(
            FN_NAMESPACE,
            "adjust-time-to-timezone",
            1,
            2,
            vec![time_opt(), day_time_duration_opt()],
            time_opt(),
        ),
    ));

    // ========================================================================
    // QName Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::ResolveQName,
        FunctionSignature::new(
            FN_NAMESPACE,
            "resolve-QName",
            vec![string_opt(), element()],
            qname_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::QName,
        FunctionSignature::new(FN_NAMESPACE, "QName", vec![string_opt(), string()], qname()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::PrefixFromQName,
        FunctionSignature::new(
            FN_NAMESPACE,
            "prefix-from-QName",
            vec![qname_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::NCName,
            )),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::LocalNameFromQName,
        FunctionSignature::new(
            FN_NAMESPACE,
            "local-name-from-QName",
            vec![qname_opt()],
            SequenceType::optional(crate::types::sequence::ItemType::AtomicType(
                crate::types::XmlTypeCode::NCName,
            )),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::NamespaceUriFromQName,
        FunctionSignature::new(
            FN_NAMESPACE,
            "namespace-uri-from-QName",
            vec![qname_opt()],
            any_uri_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::NamespaceUriForPrefix,
        FunctionSignature::new(
            FN_NAMESPACE,
            "namespace-uri-for-prefix",
            vec![string_opt(), element()],
            any_uri_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::InScopePrefixes,
        FunctionSignature::new(
            FN_NAMESPACE,
            "in-scope-prefixes",
            vec![element()],
            string_star(),
        ),
    ));

    // ========================================================================
    // URI Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::ResolveUri,
        FunctionSignature::range(
            FN_NAMESPACE,
            "resolve-uri",
            1,
            2,
            vec![string_opt(), string()],
            any_uri_opt(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::StaticBaseUri,
        FunctionSignature::new(FN_NAMESPACE, "static-base-uri", vec![], any_uri_opt()),
    ));

    // ========================================================================
    // Regex Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Matches,
        FunctionSignature::range(
            FN_NAMESPACE,
            "matches",
            2,
            3,
            vec![string_opt(), string(), string()],
            boolean(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Replace,
        FunctionSignature::range(
            FN_NAMESPACE,
            "replace",
            3,
            4,
            vec![string_opt(), string(), string(), string()],
            string(),
        ),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Tokenize,
        FunctionSignature::range(
            FN_NAMESPACE,
            "tokenize",
            2,
            3,
            vec![string_opt(), string(), string()],
            string_star(),
        ),
    ));

    // ========================================================================
    // Special Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::Trace,
        FunctionSignature::range(FN_NAMESPACE, "trace", 1, 2, vec![any(), string()], any()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Data,
        FunctionSignature::new(FN_NAMESPACE, "data", vec![any()], any_atomic_star()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::DefaultCollation,
        FunctionSignature::new(FN_NAMESPACE, "default-collation", vec![], string()),
    ));
    // F&O §3: `fn:error() as none`, `fn:error($error as xs:QName) as none`,
    // `fn:error($error as xs:QName?, $description as xs:string) as none`,
    // `fn:error($error as xs:QName?, $description as xs:string,
    // $error-object as item()*) as none`. One entry for the four arities: the
    // declared types matter only to XPath 1.0 compatibility mode, which
    // converts a single-item parameter the same way for `xs:QName` and
    // `xs:QName?`, and the implementation enforces each arity's own types
    // (see `special::error`). `none` has no SequenceType spelling; the
    // empty sequence stands for "never returns".
    registry.register(FunctionEntry::new(
        FunctionId::Error,
        FunctionSignature::range(
            FN_NAMESPACE,
            "error",
            0,
            3,
            vec![qname_opt(), string(), any()],
            empty(),
        ),
    ));

    // ========================================================================
    // Conversion Functions
    // ========================================================================
    registry.register(FunctionEntry::new(
        FunctionId::String,
        FunctionSignature::range(FN_NAMESPACE, "string", 0, 1, vec![item_opt()], string()),
    ));
    registry.register(FunctionEntry::new(
        FunctionId::Number,
        FunctionSignature::range(
            FN_NAMESPACE,
            "number",
            0,
            1,
            vec![any_atomic_opt()],
            double(),
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_lookup() {
        let entry = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "count", 1);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.id, FunctionId::Count);
    }

    #[test]
    fn test_registry_lookup_not_found() {
        let entry = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "nonexistent", 1);
        assert!(entry.is_none());
    }

    #[test]
    fn test_registry_lookup_wrong_arity() {
        let entry = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "count", 2);
        assert!(entry.is_none());
    }

    #[test]
    fn test_registry_lookup_range_arity() {
        // substring has arity 2-3
        let entry2 = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "substring", 2);
        let entry3 = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "substring", 3);
        assert!(entry2.is_some());
        assert!(entry3.is_some());
        assert_eq!(entry2.unwrap().id, FunctionId::Substring);
        assert_eq!(entry3.unwrap().id, FunctionId::Substring);
    }

    #[test]
    fn test_registry_lookup_variadic() {
        // concat is variadic with min 2
        let entry2 = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "concat", 2);
        let entry5 = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "concat", 5);
        let entry1 = FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "concat", 1);

        assert!(entry2.is_some());
        assert!(entry5.is_some());
        assert!(entry1.is_none()); // Less than min

        assert_eq!(entry2.unwrap().id, FunctionId::Concat);
        assert_eq!(entry5.unwrap().id, FunctionId::Concat);
    }

    #[test]
    fn test_registry_2010_namespace_alias() {
        let entry = FUNCTION_REGISTRY.lookup(FN_2010_NAMESPACE, "count", 1);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().id, FunctionId::Count);
    }

    #[test]
    fn test_registry_by_id() {
        let entry = FUNCTION_REGISTRY.by_id(FunctionId::Position);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().signature.local_name, "position");
    }

    /// `fn:error` is a built-in at arities 0 to 3, in both function
    /// namespaces.
    #[test]
    fn test_registry_error_is_built_in() {
        for arity in 0..=3 {
            let entry = FUNCTION_REGISTRY
                .lookup(FN_NAMESPACE, "error", arity)
                .unwrap_or_else(|| panic!("fn:error#{arity}"));
            assert_eq!(entry.id, FunctionId::Error);
            assert!(entry.signature.return_type.is_empty_sequence());
        }
        assert!(FUNCTION_REGISTRY.lookup(FN_NAMESPACE, "error", 4).is_none());
        assert_eq!(
            FUNCTION_REGISTRY
                .lookup(FN_2010_NAMESPACE, "error", 2)
                .map(|e| e.id),
            Some(FunctionId::Error)
        );
    }

    #[test]
    fn test_registry_size() {
        // We register many functions, check it's not empty
        assert!(!FUNCTION_REGISTRY.is_empty());
        assert!(FUNCTION_REGISTRY.len() > 50);
    }
}
