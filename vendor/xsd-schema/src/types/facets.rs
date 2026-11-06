//! XSD constraining facets
//!
//! This module implements the XSD facet system for constraining simple types.
//! Facets can restrict length, numeric range, pattern matching, enumeration, and whitespace.
//!
//! ## XSD Facet Categories
//!
//! - **Length facets**: length, minLength, maxLength (for string, binary, list types)
//! - **Numeric precision facets**: totalDigits, fractionDigits (for decimal types)
//! - **Bound facets**: minInclusive, maxInclusive, minExclusive, maxExclusive
//! - **String facets**: pattern, enumeration, whitespace
//! - **XSD 1.1 facets**: explicitTimezone, assertion
//!
//! ## Facet Inheritance
//!
//! When deriving a simple type by restriction:
//! - Derived facets must be more restrictive than base facets
//! - Fixed facets cannot be overridden with different values
//! - Patterns are cumulative (ANDed together)
//! - Enumerations must be subsets of base enumerations

use crate::error::{FacetError, FacetResult};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::parser::location::SourceRef;
use crate::regex_convert::lenient_ms_preprocess;
#[cfg(feature = "xsd11")]
use crate::regex_convert::rewrite_xsd10_category_escapes;
use crate::regex_convert::validate_xml_pattern_syntax;
#[cfg(not(feature = "xsd11"))]
use crate::regex_convert::{convert_xml_pattern, ConvertOptions};
use crate::schema::model::{RegexCompat, XsdVersion};
#[cfg(not(feature = "xsd11"))]
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashSet;

#[cfg(feature = "xsd11")]
use std::sync::Arc;

use super::XmlTypeCode;

/// Process-global cache of compiled XSD pattern regexes (`xsd11` / `regexml`
/// backend only).
///
/// The engine builds each Unicode category set behind `\d`, `\w` and `\p{…}`
/// once per process, but parsing a pattern and building its program still
/// costs microseconds to hundreds of microseconds — most for an XSD 1.0
/// pattern, whose category escapes [`rewrite_xsd10_category_escapes`] writes
/// out as Unicode 3.0 range text that the engine then parses range by range.
/// The same pattern strings recur constantly: the conformance harness
/// re-compiles per test, and the two-pass `xsi:schemaLocation` enrichment flow
/// re-compiles a whole schema's patterns. Caching the compiled program turns
/// those repeats into an `Arc` clone.
///
/// ## Design — direct-mapped, like a CPU cache
///
/// A **fixed-size** table of [`SLOTS`] entries. A key hashes to exactly one slot
/// (`index = hash & MASK`); a colliding key simply **overwrites** the resident
/// entry (no probing, no chaining). Properties:
///
/// - **Bounded by construction** — memory is `O(SLOTS)`, never grows, no
///   eviction-policy bookkeeping. This degrades gracefully in a long-running
///   process that sees an unbounded variety of schemas: recent patterns stay
///   resident, cold ones fall out — unlike an unbounded map, which would either
///   grow without limit or freeze at a cap and never cache anything new.
/// - **Verified hits** — each slot stores its full key; a probe is a hit only
///   when the resident key equals the lookup key, so collisions are correctness-
///   safe (they just miss and recompile).
/// - **Clearable** — [`clear_pattern_cache`] empties it (reclaim between
///   workloads, or for deterministic benchmark timings).
///
/// Keyed by everything that determines the compiled program —
/// `(pattern, xsd_version, regex_compat)`. Only **successful** compiles are
/// stored (an invalid pattern must keep reporting its error). *Distinct*
/// patterns still pay full price.
#[cfg(feature = "xsd11")]
mod pattern_cache {
    use std::sync::{Arc, OnceLock, RwLock};

    use crate::schema::model::{RegexCompat, XsdVersion};

    /// (pattern, version, compat) — everything that affects the result.
    pub(super) type Key = (String, XsdVersion, RegexCompat);

    struct Slot {
        key: Key,
        regex: Arc<regexml::Regex>,
    }

    /// Number of cache lines. Power of two so `index = hash & MASK`. At 4096
    /// the table itself is ~4096 × `size_of::<Option<Slot>>()` ≈ 130 KB plus the
    /// resident pattern strings — far smaller than the working set of *all*
    /// distinct patterns, but large enough that the heavily-repeated ones
    /// (built-ins, two-pass enrichment recompiles, shared test fixtures) rarely
    /// collide.
    const SLOTS: usize = 4096;
    const MASK: u64 = (SLOTS as u64) - 1;

    fn table() -> &'static RwLock<Box<[Option<Slot>]>> {
        static TABLE: OnceLock<RwLock<Box<[Option<Slot>]>>> = OnceLock::new();
        TABLE.get_or_init(|| {
            let slots = (0..SLOTS)
                .map(|_| None)
                .collect::<Vec<_>>()
                .into_boxed_slice();
            RwLock::new(slots)
        })
    }

    /// Slot index for a key. Fixed-seed `ahash` so a given key maps to the same
    /// line for the whole process (the seed need not be secret — the index is
    /// never exposed and collisions are correctness-safe).
    fn index(key: &Key) -> usize {
        static HASHER: OnceLock<ahash::RandomState> = OnceLock::new();
        let state = HASHER.get_or_init(|| {
            ahash::RandomState::with_seeds(
                0xa5a5_5a5a_3c3c_c3c3,
                0x1234_5678_9abc_def0,
                0x9e37_79b9_7f4a_7c15,
                0xc2b2_ae35_5d12_3456,
            )
        });
        (state.hash_one(key) & MASK) as usize
    }

    /// Look up a previously-compiled regex. A hit requires the resident slot to
    /// hold this exact key (guards against hash collisions). The lock is held
    /// only for the probe — never across a compile — so poisoning is effectively
    /// impossible; recover rather than propagate a panic.
    pub(super) fn get(key: &Key) -> Option<Arc<regexml::Regex>> {
        let idx = index(key);
        let guard = table().read().unwrap_or_else(|e| e.into_inner());
        match &guard[idx] {
            Some(slot) if &slot.key == key => Some(Arc::clone(&slot.regex)),
            _ => None,
        }
    }

    /// Record a successfully-compiled regex, replacing whatever shared its line.
    pub(super) fn store(key: Key, regex: Arc<regexml::Regex>) {
        let idx = index(&key);
        let mut guard = table().write().unwrap_or_else(|e| e.into_inner());
        guard[idx] = Some(Slot { key, regex });
    }

    /// Drop every cached entry.
    pub(super) fn clear() {
        let mut guard = table().write().unwrap_or_else(|e| e.into_inner());
        for slot in guard.iter_mut() {
            *slot = None;
        }
    }
}

/// Clear the process-global compiled-pattern regex cache.
///
/// The cache (a small fixed-size, direct-mapped table; see the module-level
/// `pattern_cache`) lives for the life of the process and speeds up recompiling
/// identical `xs:pattern` facets. Call this to reclaim it — e.g. between
/// unrelated workloads in a long-running service, or for deterministic timings
/// in a benchmark. Safe to call concurrently with compilation. Without the
/// `xsd11` feature there is no such cache and this is a no-op.
#[cfg(feature = "xsd11")]
pub fn clear_pattern_cache() {
    pattern_cache::clear();
}

/// Clear the process-global compiled-pattern regex cache. No-op without the
/// `xsd11` feature (the XSD 1.0 backend compiles via the `regex` crate and does
/// not cache).
#[cfg(not(feature = "xsd11"))]
pub fn clear_pattern_cache() {}

/// Fixed vs default facet values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FacetFixed {
    /// Value can be further restricted
    #[default]
    Default,
    /// Value cannot be changed by derived types
    Fixed,
}

/// Whitespace handling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhitespaceMode {
    /// Preserve all whitespace
    Preserve,
    /// Replace tabs/newlines with spaces
    Replace,
    /// Collapse consecutive whitespace to single space, trim
    #[default]
    Collapse,
}

/// Length facet (exact length constraint)
#[derive(Debug, Clone)]
pub struct LengthFacet {
    pub value: u64,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// MinLength facet
#[derive(Debug, Clone)]
pub struct MinLengthFacet {
    pub value: u64,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// MaxLength facet
#[derive(Debug, Clone)]
pub struct MaxLengthFacet {
    pub value: u64,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// Pattern facet (regex constraint)
#[derive(Debug, Clone)]
pub struct PatternFacet {
    /// The pattern string (XSD regex syntax)
    pub value: String,
    /// Compiled regex for efficient matching
    #[cfg(not(feature = "xsd11"))]
    compiled: Option<Regex>,
    #[cfg(feature = "xsd11")]
    compiled: Option<Arc<regexml::Regex>>,
    pub source: Option<SourceRef>,
}

impl PatternFacet {
    /// Create a new pattern facet from an XSD pattern string.
    ///
    /// The pattern is validated and compiled using the appropriate backend
    /// (without the `xsd11` feature: `regex` via `convert_xml_pattern`; with
    /// it: `regexml`, compiled once, after a `\p{X}` rewrite if
    /// `xsd_version == V1_0`). Returns an error if the pattern is invalid.
    ///
    /// `xsd_version` controls the `\p{X}` category escape semantics: under
    /// `V1_0` recognized general-category names are expanded to Unicode 3.0
    /// ranges; `V1_1` passes them through to the backend unchanged, and (with
    /// the `xsd11` feature) a block escape with an unrecognized block name
    /// denotes every character (XSD 1.1 Part 2 §G.4.2.4).
    ///
    /// `regex_compat` controls grammar leniency: `Strict` enforces XSD
    /// Part 2 §F/§G; `LenientMs` first applies [`lenient_ms_preprocess`]
    /// to drop `(?#…)` comments common in .NET-authored schemas and skips the
    /// strict grammar checks. With the `xsd11` feature, `^` and `$` are
    /// ordinary characters under `Strict`, as §F/§G define them, and anchors
    /// under `LenientMs`, whose patterns the engine reads in its XPath dialect.
    pub fn new(
        value: String,
        source: Option<SourceRef>,
        xsd_version: XsdVersion,
        regex_compat: RegexCompat,
    ) -> FacetResult<Self> {
        let mut facet = Self::new_unchecked(value, source);
        facet.compile(xsd_version, regex_compat)?;
        Ok(facet)
    }

    /// Create a pattern facet without compiling (for deferred compilation)
    pub fn new_unchecked(value: String, source: Option<SourceRef>) -> Self {
        Self {
            value,
            compiled: None,
            source,
        }
    }

    /// Compile the pattern if not already compiled
    #[cfg(not(feature = "xsd11"))]
    pub fn compile(
        &mut self,
        xsd_version: XsdVersion,
        regex_compat: RegexCompat,
    ) -> FacetResult<()> {
        if self.compiled.is_none() {
            let effective: std::borrow::Cow<'_, str> = match regex_compat {
                RegexCompat::Strict => std::borrow::Cow::Borrowed(self.value.as_str()),
                RegexCompat::LenientMs => lenient_ms_preprocess(&self.value),
            };
            // The XSD 1.0 hyphen-rule check is part of the strict §F grammar
            // gate; skip it under LenientMs so the engine alone decides.
            if xsd_version == XsdVersion::V1_0 && regex_compat == RegexCompat::Strict {
                validate_xml_pattern_syntax(&effective).map_err(|message| {
                    FacetError::InvalidPattern {
                        pattern: self.value.clone(),
                        message,
                    }
                })?;
            }
            let opts = match xsd_version {
                XsdVersion::V1_0 => ConvertOptions::xsd_v1_0(),
                XsdVersion::V1_1 => ConvertOptions::xsd(),
            };
            let rust_pattern = convert_xml_pattern(&effective, opts);
            let compiled = Regex::new(&rust_pattern).map_err(|e| FacetError::InvalidPattern {
                pattern: self.value.clone(),
                message: e.to_string(),
            })?;
            self.compiled = Some(compiled);
        }
        Ok(())
    }

    /// Compile the pattern if not already compiled
    #[cfg(feature = "xsd11")]
    pub fn compile(
        &mut self,
        xsd_version: XsdVersion,
        regex_compat: RegexCompat,
    ) -> FacetResult<()> {
        if self.compiled.is_none() {
            // Content-addressed fast path: identical (pattern, version, compat)
            // reuses the already-compiled program. See [`pattern_cache`].
            let cache_key: pattern_cache::Key = (self.value.clone(), xsd_version, regex_compat);
            if let Some(cached) = pattern_cache::get(&cache_key) {
                self.compiled = Some(cached);
                return Ok(());
            }
            let arc = Arc::new(compile_pattern_program(
                &self.value,
                xsd_version,
                regex_compat,
            )?);
            pattern_cache::store(cache_key, Arc::clone(&arc));
            self.compiled = Some(arc);
        }
        Ok(())
    }

    /// Test if a value matches this pattern
    #[cfg(not(feature = "xsd11"))]
    pub fn matches(&self, value: &str) -> bool {
        match &self.compiled {
            Some(regex) => regex.is_match(value),
            None => {
                // Defensive fallback: compile on-the-fly using XSD 1.1 defaults.
                // Reached only if a facet was never compiled via `compile_patterns`.
                if let Ok(rust_pattern) = std::panic::catch_unwind(|| {
                    convert_xml_pattern(&self.value, ConvertOptions::xsd())
                }) {
                    if let Ok(regex) = Regex::new(&rust_pattern) {
                        return regex.is_match(value);
                    }
                }
                false
            }
        }
    }

    /// Test if a value matches this pattern
    ///
    /// The whole value must match: XSD "implicitly anchors all regular
    /// expressions at the head and tail" (XSD 1.1 Part 2, Appendix G), which
    /// is the engine's `is_full_match`.
    #[cfg(feature = "xsd11")]
    pub fn matches(&self, value: &str) -> bool {
        match &self.compiled {
            Some(regex) => regex.is_full_match(value),
            None => {
                // Defensive fallback: compile on-the-fly with the XSD 1.1
                // defaults. Reached only if a facet was never compiled via
                // `compile_patterns`.
                compile_pattern_program(&self.value, XsdVersion::V1_1, RegexCompat::Strict)
                    .is_ok_and(|regex| regex.is_full_match(value))
            }
        }
    }
}

/// The flags of an XSD 1.1 pattern: none of `s`, `m`, `i`, `x` (a facet has
/// no flags), and `;k`, which makes a block escape with an unrecognized block
/// name denote every character. XSD 1.1 Part 2 §G.4.2.4: "If a string "IsX"
/// matches the non-terminal IsBlock but X is not a recognized block name, then
/// the expressions "\p{IsX}" and "\P{IsX}" each denote the set of all
/// characters." A name that does not match IsBlock (`'Is' [a-zA-Z0-9#x2D]+`)
/// stays a syntax error. XSD 1.0 has no such rule — its Appendix F lists "the
/// recognized block names" — so 1.0 patterns are compiled without the flag and
/// an unknown block stays an error.
#[cfg(feature = "xsd11")]
const XSD11_PATTERN_FLAGS: &str = ";k";

/// Compile the program of an `xs:pattern` value, once.
///
/// The program is matched against whole values with `is_full_match`, so it is
/// compiled as written — not wrapped in `^(?:…)$`:
///
/// - **`Strict`**: the XSD dialect (`Regex::xsd`). `^` and `$` are ordinary
///   characters there, as both versions of the language say: XSD 1.1 Part 2
///   §G.3, "A metacharacter is either '.', '\', '?', '*', '+', '{', '}', '(',
///   ')', '|', '[', or ']'. … A normal character is any XML character that is
///   not a ·metacharacter·", and XSD 1.0 Second Edition Appendix F, "\[10\]
///   Char ::= \[^.\\?*+()|#x5B#x5D\]". The program is also the grammar check:
///   what it rejects is not an XSD regular expression.
/// - **`LenientMs`**: the XPath dialect (`Regex::xpath`), which is what makes
///   the .NET idioms of `RegexCompat::LenientMs` work — `^` and `$` as
///   anchors, back-references, `(?:…)`, reluctant quantifiers — after
///   [`lenient_ms_preprocess`] has removed `(?#…)` comments. No XSD grammar
///   check runs.
///
/// XSD 1.0 patterns first go through the strict hyphen-rule check
/// (`validate_xml_pattern_syntax`, `Strict` only) and then through
/// [`rewrite_xsd10_category_escapes`], which pins `\p{X}`, `\d` and `\w` to
/// Unicode 3.0 by writing them out as character-class text; the rewritten text
/// is what is compiled. XSD 1.1 patterns carry [`XSD11_PATTERN_FLAGS`].
///
/// The engine's errors are reported with their `Display` text. When the
/// rewritten text of a strict XSD 1.0 pattern fails, the error of the pattern
/// as written is reported instead (the rewrite only changes category escapes,
/// so the written pattern holds the mistake; that second compile happens on
/// the error path only).
#[cfg(feature = "xsd11")]
fn compile_pattern_program(
    value: &str,
    xsd_version: XsdVersion,
    regex_compat: RegexCompat,
) -> FacetResult<regexml::Regex> {
    let invalid = |error: regexml::Error| FacetError::InvalidPattern {
        pattern: value.to_string(),
        message: error.to_string(),
    };
    let strict = regex_compat == RegexCompat::Strict;
    let effective: Cow<'_, str> = if strict {
        Cow::Borrowed(value)
    } else {
        lenient_ms_preprocess(value)
    };
    match xsd_version {
        XsdVersion::V1_0 => {
            if strict {
                validate_xml_pattern_syntax(&effective).map_err(|message| {
                    FacetError::InvalidPattern {
                        pattern: value.to_string(),
                        message,
                    }
                })?;
            }
            let pinned = rewrite_xsd10_category_escapes(&effective);
            if !strict {
                return regexml::Regex::xpath(&pinned, "").map_err(invalid);
            }
            regexml::Regex::xsd(&pinned, "").map_err(|error| {
                if pinned != *effective {
                    if let Err(written) = regexml::Regex::xsd(&effective, "") {
                        return invalid(written);
                    }
                }
                invalid(error)
            })
        }
        XsdVersion::V1_1 => if strict {
            regexml::Regex::xsd(&effective, XSD11_PATTERN_FLAGS)
        } else {
            regexml::Regex::xpath(&effective, XSD11_PATTERN_FLAGS)
        }
        .map_err(invalid),
    }
}

/// Enumeration facet (allowed values)
#[derive(Debug, Clone)]
pub struct EnumerationFacet {
    /// Set of allowed values (as strings). In a compiled schema set each is
    /// the ·normalized value· of its literal with respect to the facet's base
    /// type — white space normalized as that type prescribes (Datatypes 1.1
    /// §4.3.5.2: the values are "interpreted as instances of the {base type
    /// definition}") — so `" red "` of an `xs:token` base is stored as `red`.
    pub values: HashSet<String>,
    /// Per-value namespace bindings captured at parse time, for values that
    /// are QNames (NOTATION/QName bases). Parallel to `values`; only
    /// populated when the parser supplies a snapshot.
    pub qname_contexts: Vec<(String, NamespaceContextSnapshot)>,
    pub source: Option<SourceRef>,
}

/// An `enumeration`'s values prepared for matching: the expanded names of the
/// `xs:QName` / `xs:NOTATION` values among them (Datatypes §3.3.18 — each
/// literal read with the namespace bindings of the element it was written on,
/// [`EnumerationFacet::qname_contexts`]), and, for a union type, every
/// literal's value as the union's ordered member types give it (Datatypes
/// §4.3.5.2: the values are the literals "interpreted as instances of the
/// {base type definition}"; §2.4.1.3: a value "is validated against the
/// ·member types· in the order in which they appear in the definition until a
/// match is found"). A QName literal without recorded bindings (a facet set
/// built by hand) is kept by lexical form; one whose prefix its bindings do
/// not bind names nothing. Built once per type and cached with its effective
/// facets (`ArenasGuard::enumeration_index`), so a QName value is matched with
/// one hash lookup however long the enumeration is.
#[derive(Debug, Default)]
pub(crate) struct EnumerationIndex {
    /// `(is NOTATION, namespace, local name)` of the QName / NOTATION values.
    names: HashSet<(bool, Option<crate::ids::NameId>, crate::ids::NameId)>,
    lexical: HashSet<String>,
    /// A union type's literals and their values, one per reading of each
    /// literal.
    values: Vec<(Box<str>, super::value::XmlValue)>,
}

impl EnumerationIndex {
    /// The index of an atomic `xs:QName` (or, with `notation`, `xs:NOTATION`)
    /// type's enumeration: its literals' expanded names; their local names are
    /// interned in `names`, the table the values' ids come from.
    pub(crate) fn for_qnames(
        enumeration: &EnumerationFacet,
        names: &crate::namespace::table::NameTable,
        notation: bool,
    ) -> Self {
        let mut index = Self::default();
        for (literal, snapshot) in &enumeration.qname_contexts {
            let lexical = literal.trim();
            let (prefix, local) = match lexical.split_once(':') {
                Some((prefix, local)) => (Some(prefix), local),
                None => (None, lexical),
            };
            let namespace = match prefix {
                None | Some("") => Some(snapshot.default_ns),
                Some(prefix) => names
                    .get(prefix)
                    .and_then(|id| snapshot.resolve_prefix(id))
                    .map(Some),
            };
            if let Some(namespace) = namespace {
                index.names.insert((notation, namespace, names.add(local)));
            }
        }
        let with_bindings: HashSet<&str> = enumeration
            .qname_contexts
            .iter()
            .map(|(literal, _)| literal.as_str())
            .collect();
        for literal in &enumeration.values {
            if !with_bindings.contains(literal.as_str()) {
                index.lexical.insert(literal.clone());
            }
        }
        index
    }

    /// Add a union literal's value (as the member type that accepted it gave
    /// it): kept for comparison, and, when it is a resolved QName / NOTATION
    /// value, its expanded name.
    pub(crate) fn push_value(&mut self, literal: &str, value: super::value::XmlValue) {
        use super::value::{XmlAtomicValue, XmlValueKind};
        let mut atom = &value;
        while let XmlValueKind::Union(inner) = &atom.value {
            atom = inner;
        }
        match &atom.value {
            XmlValueKind::Atomic(XmlAtomicValue::QName(name)) => {
                let name = name.name();
                self.names
                    .insert((false, name.namespace_uri, name.local_name));
            }
            XmlValueKind::Atomic(XmlAtomicValue::Notation(name)) => {
                let name = name.name();
                self.names
                    .insert((true, name.namespace_uri, name.local_name));
            }
            _ => {}
        }
        self.values.push((literal.into(), value));
    }

    /// Add a QName / NOTATION literal kept by lexical form.
    pub(crate) fn push_lexical(&mut self, literal: &str) {
        self.lexical.insert(literal.to_string());
    }

    /// Whether the QName (`notation`: NOTATION) value `{namespace}local` (its
    /// local name's id), of lexical form `normalized`, is enumerated.
    pub(crate) fn admits(
        &self,
        normalized: &str,
        notation: bool,
        namespace: Option<crate::ids::NameId>,
        local: crate::ids::NameId,
    ) -> bool {
        self.names.contains(&(notation, namespace, local)) || self.lexical.contains(normalized)
    }

    /// Whether the expanded name `{namespace}local` is enumerated, by name only.
    pub(crate) fn has_name(
        &self,
        notation: bool,
        namespace: Option<crate::ids::NameId>,
        local: crate::ids::NameId,
    ) -> bool {
        self.names.contains(&(notation, namespace, local))
    }

    /// A union type's literals and their values (empty for an atomic type).
    pub(crate) fn values(&self) -> &[(Box<str>, super::value::XmlValue)] {
        &self.values
    }
}

/// Whitespace facet
#[derive(Debug, Clone)]
pub struct WhitespaceFacet {
    pub value: WhitespaceMode,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// MinInclusive facet (value >= bound)
#[derive(Debug, Clone)]
pub struct MinInclusiveFacet {
    /// The bound as a string (type-specific interpretation during validation)
    pub value: String,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// MaxInclusive facet (value <= bound)
#[derive(Debug, Clone)]
pub struct MaxInclusiveFacet {
    pub value: String,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// MinExclusive facet (value > bound)
#[derive(Debug, Clone)]
pub struct MinExclusiveFacet {
    pub value: String,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// MaxExclusive facet (value < bound)
#[derive(Debug, Clone)]
pub struct MaxExclusiveFacet {
    pub value: String,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// TotalDigits facet (for decimal types)
#[derive(Debug, Clone)]
pub struct TotalDigitsFacet {
    pub value: u32,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// FractionDigits facet (decimal places)
#[derive(Debug, Clone)]
pub struct FractionDigitsFacet {
    pub value: u32,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// XSD 1.1: Assertion facet (XPath constraint on simple type values)
#[derive(Debug, Clone)]
pub struct AssertionFacet {
    /// XPath 2.0 test expression
    pub test: String,
    /// Raw xpathDefaultNamespace attribute (resolved at evaluation time)
    pub xpath_default_namespace: Option<String>,
    /// Namespace bindings snapshot at parse time (for prefix resolution in XPath)
    pub ns_snapshot: NamespaceContextSnapshot,
    pub source: Option<SourceRef>,
}

/// XSD 1.1: ExplicitTimezone facet
/// TODO: XSD 1.1 - Implement explicitTimezone constraint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplicitTimezone {
    Required,
    Prohibited,
    Optional,
}

/// XSD 1.1: ExplicitTimezone facet data
#[derive(Debug, Clone)]
pub struct ExplicitTimezoneFacet {
    pub value: ExplicitTimezone,
    pub fixed: FacetFixed,
    pub source: Option<SourceRef>,
}

/// Complete set of facets for a simple type
///
/// A FacetSet collects all constraining facets that apply to a simple type.
/// Facets are accumulated during type derivation.
#[derive(Debug, Clone, Default)]
pub struct FacetSet {
    // String length facets
    pub length: Option<LengthFacet>,
    pub min_length: Option<MinLengthFacet>,
    pub max_length: Option<MaxLengthFacet>,

    // Pattern facets grouped by derivation step.
    //
    // Per XSD Datatypes Part 2 §4.3.4 (and the equivalent §A.2 prose),
    // multiple `<xs:pattern>` facets in a single restriction step combine
    // as alternation (logical OR), while patterns inherited from earlier
    // derivation steps further restrict the value (logical AND).
    //
    // Outer Vec = AND across derivation steps; inner Vec = OR within a step.
    pub patterns: Vec<Vec<PatternFacet>>,

    // Enumeration (allowed values). The `Option` is only the presence flag;
    // multi-valued semantics live inside `EnumerationFacet::values` (HashSet),
    // so enumeration is exempt from st-props-correct.1 "no duplicate facet" (§3.16.2).
    pub enumeration: Option<EnumerationFacet>,

    // Whitespace handling
    pub whitespace: Option<WhitespaceFacet>,

    // Numeric range facets
    pub min_inclusive: Option<MinInclusiveFacet>,
    pub max_inclusive: Option<MaxInclusiveFacet>,
    pub min_exclusive: Option<MinExclusiveFacet>,
    pub max_exclusive: Option<MaxExclusiveFacet>,

    // Decimal precision facets
    pub total_digits: Option<TotalDigitsFacet>,
    pub fraction_digits: Option<FractionDigitsFacet>,

    // XSD 1.1 facets
    // TODO: XSD 1.1 - These are parsed but not enforced in 1.0 mode
    pub assertions: Vec<AssertionFacet>,
    pub explicit_timezone: Option<ExplicitTimezoneFacet>,
}

impl FacetSet {
    /// Create a new empty facet set
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the facet set is empty (no facets defined)
    pub fn is_empty(&self) -> bool {
        self.length.is_none()
            && self.min_length.is_none()
            && self.max_length.is_none()
            && self.patterns.iter().all(|step| step.is_empty())
            && self.enumeration.is_none()
            && self.whitespace.is_none()
            && self.min_inclusive.is_none()
            && self.max_inclusive.is_none()
            && self.min_exclusive.is_none()
            && self.max_exclusive.is_none()
            && self.total_digits.is_none()
            && self.fraction_digits.is_none()
            && self.assertions.is_empty()
            && self.explicit_timezone.is_none()
    }

    /// Set length facet
    pub fn set_length(&mut self, value: u64, fixed: FacetFixed, source: Option<SourceRef>) {
        self.length = Some(LengthFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set minLength facet
    pub fn set_min_length(&mut self, value: u64, fixed: FacetFixed, source: Option<SourceRef>) {
        self.min_length = Some(MinLengthFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set maxLength facet
    pub fn set_max_length(&mut self, value: u64, fixed: FacetFixed, source: Option<SourceRef>) {
        self.max_length = Some(MaxLengthFacet {
            value,
            fixed,
            source,
        });
    }

    /// Add a pattern facet (compiles the pattern) at the current
    /// derivation step. Multiple consecutive `add_pattern` calls on the
    /// same FacetSet are treated as alternatives (OR'd) within a single
    /// step; a new step is opened by `inherit_from` / `merge_with_base`.
    pub fn add_pattern(
        &mut self,
        value: String,
        source: Option<SourceRef>,
        xsd_version: XsdVersion,
        regex_compat: RegexCompat,
    ) -> FacetResult<()> {
        let pattern = PatternFacet::new(value, source, xsd_version, regex_compat)?;
        self.push_pattern_to_current_step(pattern);
        Ok(())
    }

    /// Add a pattern facet without compiling (for deferred validation),
    /// at the current derivation step. See `add_pattern` for the OR/AND
    /// semantics across multiple calls.
    pub fn add_pattern_unchecked(&mut self, value: String, source: Option<SourceRef>) {
        self.push_pattern_to_current_step(PatternFacet::new_unchecked(value, source));
    }

    fn push_pattern_to_current_step(&mut self, pattern: PatternFacet) {
        if self.patterns.is_empty() {
            self.patterns.push(Vec::new());
        }
        self.patterns.last_mut().unwrap().push(pattern);
    }

    /// Compile all uncompiled patterns. Returns the first error encountered.
    ///
    /// `xsd_version` selects the Unicode-category semantics for `\p{X}`: V1_0
    /// pins to Unicode 3.0; V1_1 passes through to the backend.
    /// `regex_compat` controls grammar leniency (see [`PatternFacet::compile`]).
    pub fn compile_patterns(
        &mut self,
        xsd_version: XsdVersion,
        regex_compat: RegexCompat,
    ) -> FacetResult<()> {
        for step in &mut self.patterns {
            for pattern in step {
                pattern.compile(xsd_version, regex_compat)?;
            }
        }
        Ok(())
    }

    /// Run the per-step XSD pattern check on `value`: every step must have
    /// at least one alternative that matches (within-step OR, across-step
    /// AND). Returns the first failing step's first pattern as error
    /// context.
    fn check_patterns(&self, value: &str) -> FacetResult<()> {
        for step in &self.patterns {
            if step.is_empty() {
                continue;
            }
            if !step.iter().any(|p| p.matches(value)) {
                return Err(FacetError::pattern(value, &step[0].value));
            }
        }
        Ok(())
    }

    /// Add an enumeration value
    pub fn add_enumeration(&mut self, value: String, source: Option<SourceRef>) {
        self.add_enumeration_with_ns(value, None, source);
    }

    /// Add an enumeration value, capturing the namespace bindings in scope
    /// at the `<xs:enumeration>` element (needed to resolve QName-valued
    /// enumerations against NOTATION/QName bases).
    pub fn add_enumeration_with_ns(
        &mut self,
        value: String,
        ns_snapshot: Option<NamespaceContextSnapshot>,
        source: Option<SourceRef>,
    ) {
        let enumeration = self.enumeration.get_or_insert_with(|| EnumerationFacet {
            values: HashSet::new(),
            qname_contexts: Vec::new(),
            source: source.clone(),
        });
        if let Some(snapshot) = ns_snapshot {
            enumeration.qname_contexts.push((value.clone(), snapshot));
        }
        enumeration.values.insert(value);
    }

    /// Set whitespace facet
    pub fn set_whitespace(
        &mut self,
        value: WhitespaceMode,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.whitespace = Some(WhitespaceFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set minInclusive facet
    pub fn set_min_inclusive(
        &mut self,
        value: String,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.min_inclusive = Some(MinInclusiveFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set maxInclusive facet
    pub fn set_max_inclusive(
        &mut self,
        value: String,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.max_inclusive = Some(MaxInclusiveFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set minExclusive facet
    pub fn set_min_exclusive(
        &mut self,
        value: String,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.min_exclusive = Some(MinExclusiveFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set maxExclusive facet
    pub fn set_max_exclusive(
        &mut self,
        value: String,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.max_exclusive = Some(MaxExclusiveFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set totalDigits facet
    pub fn set_total_digits(&mut self, value: u32, fixed: FacetFixed, source: Option<SourceRef>) {
        self.total_digits = Some(TotalDigitsFacet {
            value,
            fixed,
            source,
        });
    }

    /// Set fractionDigits facet
    pub fn set_fraction_digits(
        &mut self,
        value: u32,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.fraction_digits = Some(FractionDigitsFacet {
            value,
            fixed,
            source,
        });
    }

    /// Add an assertion facet (XSD 1.1)
    pub fn add_assertion(
        &mut self,
        test: String,
        xpath_default_namespace: Option<String>,
        ns_snapshot: NamespaceContextSnapshot,
        source: Option<SourceRef>,
    ) {
        self.assertions.push(AssertionFacet {
            test,
            xpath_default_namespace,
            ns_snapshot,
            source,
        });
    }

    /// Set explicitTimezone facet (XSD 1.1)
    pub fn set_explicit_timezone(
        &mut self,
        value: ExplicitTimezone,
        fixed: FacetFixed,
        source: Option<SourceRef>,
    ) {
        self.explicit_timezone = Some(ExplicitTimezoneFacet {
            value,
            fixed,
            source,
        });
    }

    /// Merge facets from a base type (for type derivation by restriction)
    ///
    /// Inherited facets are only set if not already defined in this facet set
    /// (Structures 1.1 §3.16.6.4 clause 3: "The {facets} of R are the {facets}
    /// of B ·overlaid· with S"); patterns and assertions accumulate. An
    /// `enumeration` of this set replaces the base's, whose values it must be
    /// drawn from; without one, the base's is kept, with the namespace
    /// bindings of its QName values. The `fixed` attribute is preserved from
    /// the base type.
    ///
    /// Note: This method does not validate that derived facets are more restrictive.
    /// Use `merge_with_base()` for full validation.
    pub fn inherit_from(&mut self, base: &FacetSet) {
        // String length facets
        if self.length.is_none() {
            self.length = base.length.clone();
        }
        if self.min_length.is_none() {
            self.min_length = base.min_length.clone();
        }
        if self.max_length.is_none() {
            self.max_length = base.max_length.clone();
        }

        // Each base derivation step is appended as a new outer step on
        // self, preserving the within-step OR / across-step AND structure.
        // (Step-level dedup vs. derived's local step would require value-set
        // equivalence checks; skip it — repeated identical patterns are
        // idempotent under either OR or AND, and recompilation cost is
        // bounded by `compile_patterns` running per FacetSet at most once.)
        for base_step in &base.patterns {
            if !base_step.is_empty() {
                self.patterns.push(base_step.clone());
            }
        }

        if self.enumeration.is_none() {
            self.enumeration = base.enumeration.clone();
        }

        // Whitespace
        if self.whitespace.is_none() {
            self.whitespace = base.whitespace.clone();
        }

        // Numeric bounds
        if self.min_inclusive.is_none() {
            self.min_inclusive = base.min_inclusive.clone();
        }
        if self.max_inclusive.is_none() {
            self.max_inclusive = base.max_inclusive.clone();
        }
        if self.min_exclusive.is_none() {
            self.min_exclusive = base.min_exclusive.clone();
        }
        if self.max_exclusive.is_none() {
            self.max_exclusive = base.max_exclusive.clone();
        }

        // Decimal precision
        if self.total_digits.is_none() {
            self.total_digits = base.total_digits.clone();
        }
        if self.fraction_digits.is_none() {
            self.fraction_digits = base.fraction_digits.clone();
        }

        // XSD 1.1 assertions are cumulative
        for assertion in &base.assertions {
            self.assertions.push(assertion.clone());
        }

        if self.explicit_timezone.is_none() {
            self.explicit_timezone = base.explicit_timezone.clone();
        }
    }

    /// Merge base type facets with derived type facets, validating derivation rules.
    ///
    /// This method enforces XSD derivation by restriction rules:
    /// - Fixed facets cannot be overridden with different values
    /// - Derived facets must be more restrictive than base facets
    /// - Patterns are cumulative (ANDed together)
    /// - Enumerations must be subsets of base enumerations
    ///
    /// Returns a new FacetSet combining base and derived facets, or an error
    /// if the derivation rules are violated.
    ///
    /// `datatype` is the built-in datatype whose value space the facet values
    /// are in — the primitive, or any built-in type derived from it, of the
    /// type the facets restrict (Datatypes 1.1 §4.3.5.1 / §4.3.7.1: {value}
    /// is "A value from the ·value space· of the {base type definition}").
    /// The facet values are read with its lexical mapping and compared as
    /// values:
    ///
    /// - a `fixed` bound (§4.3.7.1: "cannot specify a value for maxInclusive
    ///   other than {value}") is matched by any literal of the same value —
    ///   `10.0` for a decimal `10`, `2000-01-01+00:00` for `2000-01-01Z`;
    /// - a lower bound is checked against an upper one in the datatype's
    ///   order (§4.3.7.4 – §4.3.10.4) — exactly for `xs:decimal` and its
    ///   restrictions, in the float / double value space for those, in the
    ///   partial orders of the date/time and duration datatypes, where two
    ///   ·incomparable· bounds are not out of order; `minExclusive` may equal
    ///   `maxExclusive`;
    /// - each bound is checked against every bound of `base` by the four
    ///   "valid restriction" constraints (§4.3.7.4 – §4.3.10.4), and a
    ///   `length` must equal the base's (§4.3.1.4 length valid restriction);
    /// - each derived `enumeration` value must be equal or identical to a
    ///   value of the base's (§4.3.5.5).
    ///
    /// `base` is the base type's {facets}: its own and those it inherits. A
    /// literal the datatype's lexical mapping does not accept, and every
    /// literal when `datatype` has no registered validator (such as
    /// `XmlTypeCode::AnySimpleType`, for a list or union type), is compared
    /// as written, and such bounds are not ordered. `xs:QName` and
    /// `xs:NOTATION` literals are compared as written too: without the
    /// namespace bindings they were written with, `p:a` and `q:a` differ even
    /// when `p` and `q` name one namespace (the schema compiler compares them
    /// by expanded name itself). Two date/time bounds with years on both
    /// sides of year 1 are not ordered: XSD 1.0 and 1.1 number those years
    /// differently. Whether each facet value is itself in the base type's
    /// value space is not checked here.
    pub fn merge_with_base(&self, base: &FacetSet, datatype: XmlTypeCode) -> FacetResult<FacetSet> {
        // Datatypes 1.1 §4.3.8.4 / §4.3.9.4 (XSD 1.0 alike): "It is an
        // ·error· for both ·maxInclusive· and ·maxExclusive· to be specified
        // in the same derivation step" — this step's own facets; a base may
        // carry both from different steps (Structures §3.16.6.4 overlays
        // facets of the same kind only), and then both bound its values.
        if self.min_inclusive.is_some() && self.min_exclusive.is_some() {
            return Err(FacetError::conflicting(
                "cannot have both minInclusive and minExclusive",
            ));
        }
        if self.max_inclusive.is_some() && self.max_exclusive.is_some() {
            return Err(FacetError::conflicting(
                "cannot have both maxInclusive and maxExclusive",
            ));
        }
        // XSD Datatypes Part 2 §4.3.1.4 "length and minLength or maxLength":
        // both `length` and `minLength` in {facets} is an error UNLESS
        // (1.1) minLength <= length AND (1.2) some restriction ancestor has
        // minLength with the same {value} and no length. The base's merged
        // facet set stands in for "some ancestor": if it carries the same
        // minLength without a length, clause 1.2 holds (e.g. xs:IDREFS whose
        // built-in minLength=1 licenses re-stating minLength=1 next to a new
        // length — W3C bug 6446, msData IDREFS_length006). Symmetrically for
        // maxLength via clauses 2.1/2.2.
        if let (Some(len), Some(min)) = (&self.length, &self.min_length) {
            let permitted = min.value <= len.value
                && base.length.is_none()
                && base
                    .min_length
                    .as_ref()
                    .is_some_and(|b| b.value == min.value);
            if !permitted {
                return Err(FacetError::conflicting(
                    "length and minLength cannot both appear in the same restriction step",
                ));
            }
        }
        if let (Some(len), Some(max)) = (&self.length, &self.max_length) {
            let permitted = len.value <= max.value
                && base.length.is_none()
                && base
                    .max_length
                    .as_ref()
                    .is_some_and(|b| b.value == max.value);
            if !permitted {
                return Err(FacetError::conflicting(
                    "length and maxLength cannot both appear in the same restriction step",
                ));
            }
        }

        self.check_bound_restrictions(base, datatype)?;

        let mut result = self.clone();

        // === Length facets ===
        // Validate and merge length facet
        if let Some(ref base_length) = base.length {
            match &result.length {
                Some(derived) => {
                    // Fixed length cannot be changed
                    if base_length.fixed == FacetFixed::Fixed && derived.value != base_length.value
                    {
                        return Err(FacetError::fixed_violation(
                            "length",
                            base_length.value.to_string(),
                            derived.value.to_string(),
                        ));
                    }
                    // Datatypes §4.3.1.4 length valid restriction (XSD 1.0
                    // alike): "It is an ·error· if length is among the members
                    // of {facets} of {base type definition} and {value} is not
                    // equal to the {value} of the parent length" — fixed or not.
                    if derived.value != base_length.value {
                        return Err(FacetError::derivation(format!(
                            "length {} differs from base length {} (length valid restriction)",
                            derived.value, base_length.value
                        )));
                    }
                }
                None => {
                    result.length = Some(base_length.clone());
                }
            }
        }

        // Validate and merge minLength facet
        if let Some(ref base_min) = base.min_length {
            match &result.min_length {
                Some(derived) => {
                    if base_min.fixed == FacetFixed::Fixed && derived.value != base_min.value {
                        return Err(FacetError::fixed_violation(
                            "minLength",
                            base_min.value.to_string(),
                            derived.value.to_string(),
                        ));
                    }
                    // Derived minLength must be >= base minLength
                    if derived.value < base_min.value {
                        return Err(FacetError::derivation(format!(
                            "minLength {} is less restrictive than base minLength {}",
                            derived.value, base_min.value
                        )));
                    }
                }
                None => {
                    result.min_length = Some(base_min.clone());
                }
            }
        }

        // Validate and merge maxLength facet
        if let Some(ref base_max) = base.max_length {
            match &result.max_length {
                Some(derived) => {
                    if base_max.fixed == FacetFixed::Fixed && derived.value != base_max.value {
                        return Err(FacetError::fixed_violation(
                            "maxLength",
                            base_max.value.to_string(),
                            derived.value.to_string(),
                        ));
                    }
                    // Derived maxLength must be <= base maxLength
                    if derived.value > base_max.value {
                        return Err(FacetError::derivation(format!(
                            "maxLength {} is less restrictive than base maxLength {}",
                            derived.value, base_max.value
                        )));
                    }
                }
                None => {
                    result.max_length = Some(base_max.clone());
                }
            }
        }

        // === Patterns ===
        // Each base derivation step is appended as a new outer step on
        // result. Within-step OR / across-step AND semantics survive the
        // merge per XSD Datatypes §4.3.4.
        for base_step in &base.patterns {
            if !base_step.is_empty() {
                result.patterns.push(base_step.clone());
            }
        }

        // === Enumeration ===
        // If base has enumeration, derived must be a subset (or not specify enumeration)
        if let Some(ref base_enum) = base.enumeration {
            match &result.enumeration {
                Some(derived_enum) => {
                    // Check that derived values are subset of base values
                    for value in &derived_enum.values {
                        if !base_enum.values.contains(value)
                            && !base_enum
                                .values
                                .iter()
                                .any(|member| literal_values_equal(datatype, value, member))
                        {
                            return Err(FacetError::derivation(format!(
                                "enumeration value '{}' is not in base enumeration",
                                value
                            )));
                        }
                    }
                }
                None => {
                    // Inherit base enumeration
                    result.enumeration = Some(base_enum.clone());
                }
            }
        }

        // === Whitespace ===
        if let Some(ref base_ws) = base.whitespace {
            match &result.whitespace {
                Some(derived) => {
                    if base_ws.fixed == FacetFixed::Fixed && derived.value != base_ws.value {
                        return Err(FacetError::fixed_violation(
                            "whiteSpace",
                            format!("{:?}", base_ws.value),
                            format!("{:?}", derived.value),
                        ));
                    }
                    // Whitespace can only become more restrictive:
                    // preserve -> replace -> collapse
                    if !is_whitespace_more_restrictive(derived.value, base_ws.value) {
                        return Err(FacetError::derivation(format!(
                            "whiteSpace {:?} is less restrictive than base {:?}",
                            derived.value, base_ws.value
                        )));
                    }
                }
                None => {
                    result.whitespace = Some(base_ws.clone());
                }
            }
        }

        // === Numeric bounds ===
        // Note: Full numeric comparison would require parsing the values
        // For now, we check fixed constraints and inherit missing values.
        //
        // A derived type may switch between Inclusive and Exclusive for the same bound
        // (e.g., base has minInclusive, derived has minExclusive).  Per cos-st-restricts,
        // only the derived facet applies, so we must NOT inherit the base facet when the
        // derived type already supplies the complementary one.
        if let Some(ref base_facet) = base.min_inclusive {
            if let Some(ref derived) = result.min_inclusive {
                if base_facet.fixed == FacetFixed::Fixed
                    && !literal_values_equal(datatype, &derived.value, &base_facet.value)
                {
                    return Err(FacetError::fixed_violation(
                        "minInclusive",
                        &base_facet.value,
                        &derived.value,
                    ));
                }
            } else if result.min_exclusive.is_none() {
                // Only inherit if derived hasn't replaced it with minExclusive
                result.min_inclusive = Some(base_facet.clone());
            }
        }

        if let Some(ref base_facet) = base.max_inclusive {
            if let Some(ref derived) = result.max_inclusive {
                if base_facet.fixed == FacetFixed::Fixed
                    && !literal_values_equal(datatype, &derived.value, &base_facet.value)
                {
                    return Err(FacetError::fixed_violation(
                        "maxInclusive",
                        &base_facet.value,
                        &derived.value,
                    ));
                }
            } else if result.max_exclusive.is_none() {
                // Only inherit if derived hasn't replaced it with maxExclusive
                result.max_inclusive = Some(base_facet.clone());
            }
        }

        if let Some(ref base_facet) = base.min_exclusive {
            if let Some(ref derived) = result.min_exclusive {
                if base_facet.fixed == FacetFixed::Fixed
                    && !literal_values_equal(datatype, &derived.value, &base_facet.value)
                {
                    return Err(FacetError::fixed_violation(
                        "minExclusive",
                        &base_facet.value,
                        &derived.value,
                    ));
                }
            } else if result.min_inclusive.is_none() {
                // Only inherit if derived hasn't replaced it with minInclusive
                result.min_exclusive = Some(base_facet.clone());
            }
        }

        if let Some(ref base_facet) = base.max_exclusive {
            if let Some(ref derived) = result.max_exclusive {
                if base_facet.fixed == FacetFixed::Fixed
                    && !literal_values_equal(datatype, &derived.value, &base_facet.value)
                {
                    return Err(FacetError::fixed_violation(
                        "maxExclusive",
                        &base_facet.value,
                        &derived.value,
                    ));
                }
            } else if result.max_inclusive.is_none() {
                // Only inherit if derived hasn't replaced it with maxInclusive
                result.max_exclusive = Some(base_facet.clone());
            }
        }

        // === Digit facets ===
        if let Some(ref base_td) = base.total_digits {
            match &result.total_digits {
                Some(derived) => {
                    if base_td.fixed == FacetFixed::Fixed && derived.value != base_td.value {
                        return Err(FacetError::fixed_violation(
                            "totalDigits",
                            base_td.value.to_string(),
                            derived.value.to_string(),
                        ));
                    }
                    // Derived totalDigits must be <= base totalDigits
                    if derived.value > base_td.value {
                        return Err(FacetError::derivation(format!(
                            "totalDigits {} is less restrictive than base totalDigits {}",
                            derived.value, base_td.value
                        )));
                    }
                }
                None => {
                    result.total_digits = Some(base_td.clone());
                }
            }
        }

        if let Some(ref base_fd) = base.fraction_digits {
            match &result.fraction_digits {
                Some(derived) => {
                    if base_fd.fixed == FacetFixed::Fixed && derived.value != base_fd.value {
                        return Err(FacetError::fixed_violation(
                            "fractionDigits",
                            base_fd.value.to_string(),
                            derived.value.to_string(),
                        ));
                    }
                    // Derived fractionDigits must be <= base fractionDigits
                    if derived.value > base_fd.value {
                        return Err(FacetError::derivation(format!(
                            "fractionDigits {} is less restrictive than base fractionDigits {}",
                            derived.value, base_fd.value
                        )));
                    }
                }
                None => {
                    result.fraction_digits = Some(base_fd.clone());
                }
            }
        }

        // === XSD 1.1 facets ===
        // Assertions are cumulative
        for assertion in &base.assertions {
            result.assertions.push(assertion.clone());
        }

        // ExplicitTimezone — §4.3.16 Valid explicitTimezone Restrictions:
        //   base=optional   → derived ∈ {optional, required, prohibited}
        //   base=required   → derived ∈ {required}
        //   base=prohibited → derived ∈ {prohibited}
        // This restriction is independent of {fixed}; fixed adds only a
        // stronger value-equality requirement on top.
        if let Some(ref base_etz) = base.explicit_timezone {
            if let Some(ref derived) = result.explicit_timezone {
                let restriction_ok = match base_etz.value {
                    ExplicitTimezone::Optional => true,
                    ExplicitTimezone::Required => derived.value == ExplicitTimezone::Required,
                    ExplicitTimezone::Prohibited => derived.value == ExplicitTimezone::Prohibited,
                };
                if !restriction_ok {
                    return Err(FacetError::derivation(format!(
                        "explicitTimezone {:?} is not a valid restriction of base {:?}",
                        derived.value, base_etz.value
                    )));
                }
                if base_etz.fixed == FacetFixed::Fixed && derived.value != base_etz.value {
                    return Err(FacetError::fixed_violation(
                        "explicitTimezone",
                        format!("{:?}", base_etz.value),
                        format!("{:?}", derived.value),
                    ));
                }
            } else {
                result.explicit_timezone = Some(base_etz.clone());
            }
        }

        // === Validate conflicting facets ===
        result.validate_consistency(datatype)?;

        Ok(result)
    }

    /// The four "valid restriction" constraints of the bounding facets
    /// (Datatypes 1.1 §4.3.7.4 – §4.3.10.4): each bound of this step against
    /// every bound among the base's {facets}, in `datatype`'s order — also a
    /// base bound of the opposite kind that this step's own bound replaces in
    /// the merged set (`maxExclusive 0` under the `minInclusive 0` of
    /// `xs:unsignedLong`, clause 3: "less than or equal to the {value} of that
    /// minInclusive"). ·Incomparable· values break no clause. The 1.1 tables
    /// are used in both versions: where XSD 1.0's minExclusive valid
    /// restriction lacks 1.1's clause 3 ("greater than or equal to the
    /// {value} of that maxInclusive"), its "minExclusive < maxInclusive … for
    /// the same datatype" forbids the same value, the inherited maxInclusive
    /// being among the datatype's {facets}.
    fn check_bound_restrictions(&self, base: &FacetSet, datatype: XmlTypeCode) -> FacetResult<()> {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let derived = [
            (
                "minInclusive",
                self.min_inclusive.as_ref().map(|f| &f.value),
            ),
            (
                "maxInclusive",
                self.max_inclusive.as_ref().map(|f| &f.value),
            ),
            (
                "minExclusive",
                self.min_exclusive.as_ref().map(|f| &f.value),
            ),
            (
                "maxExclusive",
                self.max_exclusive.as_ref().map(|f| &f.value),
            ),
        ];
        let base_bounds = [
            (
                "minInclusive",
                base.min_inclusive.as_ref().map(|f| &f.value),
            ),
            (
                "maxInclusive",
                base.max_inclusive.as_ref().map(|f| &f.value),
            ),
            (
                "minExclusive",
                base.min_exclusive.as_ref().map(|f| &f.value),
            ),
            (
                "maxExclusive",
                base.max_exclusive.as_ref().map(|f| &f.value),
            ),
        ];
        for (kind, value) in derived {
            let Some(value) = value else { continue };
            for (base_kind, base_value) in base_bounds {
                let Some(base_value) = base_value else {
                    continue;
                };
                let Some(order) = literal_order(datatype, value, base_value) else {
                    continue;
                };
                // The clause tables: the orders of {value} against the base
                // facet that are an error.
                let error = match (kind, base_kind) {
                    ("maxInclusive", "maxInclusive") => order == Greater,
                    ("maxInclusive", "maxExclusive") => order != Less,
                    ("maxInclusive", "minInclusive") => order == Less,
                    ("maxInclusive", "minExclusive") => order != Greater,
                    ("maxExclusive", "maxExclusive") => order == Greater,
                    ("maxExclusive", "maxInclusive") => order == Greater,
                    ("maxExclusive", "minInclusive") => order != Greater,
                    ("maxExclusive", "minExclusive") => order != Greater,
                    ("minExclusive", "minExclusive") => order == Less,
                    ("minExclusive", "minInclusive") => order == Less,
                    ("minExclusive", "maxInclusive") => order != Less,
                    ("minExclusive", "maxExclusive") => order != Less,
                    ("minInclusive", "minInclusive") => order == Less,
                    ("minInclusive", "maxInclusive") => order == Greater,
                    ("minInclusive", "minExclusive") => order != Greater,
                    ("minInclusive", "maxExclusive") => order != Less,
                    _ => false,
                };
                if error {
                    let relation = match order {
                        Less => "less than",
                        Equal => "equal to",
                        Greater => "greater than",
                    };
                    return Err(FacetError::derivation(format!(
                        "{kind} '{value}' is {relation} the base's {base_kind} '{base_value}' \
                         ({kind} valid restriction)"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Validate internal consistency of facets
    fn validate_consistency(&self, datatype: XmlTypeCode) -> FacetResult<()> {
        // Check minLength <= maxLength
        if let (Some(min), Some(max)) = (&self.min_length, &self.max_length) {
            if min.value > max.value {
                return Err(FacetError::conflicting(format!(
                    "minLength {} is greater than maxLength {}",
                    min.value, max.value
                )));
            }
        }

        // Check length conflicts with minLength/maxLength
        if let Some(len) = &self.length {
            if let Some(min) = &self.min_length {
                if len.value < min.value {
                    return Err(FacetError::conflicting(format!(
                        "length {} is less than minLength {}",
                        len.value, min.value
                    )));
                }
            }
            if let Some(max) = &self.max_length {
                if len.value > max.value {
                    return Err(FacetError::conflicting(format!(
                        "length {} is greater than maxLength {}",
                        len.value, max.value
                    )));
                }
            }
        }

        // Check fractionDigits <= totalDigits
        if let (Some(fd), Some(td)) = (&self.fraction_digits, &self.total_digits) {
            if fd.value > td.value {
                return Err(FacetError::conflicting(format!(
                    "fractionDigits {} is greater than totalDigits {}",
                    fd.value, td.value
                )));
            }
        }

        // Check numeric bound consistency (minInclusive vs maxInclusive, etc.)
        // Uses decimal parsing for numeric comparison
        if let (Some(min_incl), Some(max_incl)) = (&self.min_inclusive, &self.max_inclusive) {
            if let Some(cmp) = literal_order(datatype, &min_incl.value, &max_incl.value) {
                if cmp == std::cmp::Ordering::Greater {
                    return Err(FacetError::conflicting(format!(
                        "minInclusive '{}' is greater than maxInclusive '{}'",
                        min_incl.value, max_incl.value
                    )));
                }
            }
        }
        // Datatypes 1.1 §4.3.8.4 (XSD 1.0 alike), "minExclusive <=
        // maxExclusive": "It is an ·error· for the value specified for
        // ·minExclusive· to be greater than the value specified for
        // ·maxExclusive·" — equal is allowed (the value space is empty).
        if let (Some(min_excl), Some(max_excl)) = (&self.min_exclusive, &self.max_exclusive) {
            if let Some(cmp) = literal_order(datatype, &min_excl.value, &max_excl.value) {
                if cmp == std::cmp::Ordering::Greater {
                    return Err(FacetError::conflicting(format!(
                        "minExclusive '{}' is greater than maxExclusive '{}'",
                        min_excl.value, max_excl.value
                    )));
                }
            }
        }
        if let (Some(min_incl), Some(max_excl)) = (&self.min_inclusive, &self.max_exclusive) {
            if let Some(cmp) = literal_order(datatype, &min_incl.value, &max_excl.value) {
                if cmp != std::cmp::Ordering::Less {
                    return Err(FacetError::conflicting(format!(
                        "minInclusive '{}' must be less than maxExclusive '{}'",
                        min_incl.value, max_excl.value
                    )));
                }
            }
        }
        if let (Some(min_excl), Some(max_incl)) = (&self.min_exclusive, &self.max_inclusive) {
            if let Some(cmp) = literal_order(datatype, &min_excl.value, &max_incl.value) {
                if cmp != std::cmp::Ordering::Less {
                    return Err(FacetError::conflicting(format!(
                        "minExclusive '{}' must be less than maxInclusive '{}'",
                        min_excl.value, max_incl.value
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate a string value against all applicable facets
    ///
    /// This validates length, pattern, enumeration, and whitespace facets.
    /// Numeric bounds and digit facets require parsed values and are not
    /// validated by this method.
    pub fn validate_string(&self, value: &str) -> FacetResult<()> {
        // Apply whitespace normalization for length calculation
        let normalized = match &self.whitespace {
            Some(ws) => normalize_whitespace(value, ws.value),
            None => Cow::Borrowed(value),
        };
        let check_value: &str = &normalized;

        // Check length facet
        if let Some(ref length) = self.length {
            let len = check_value.chars().count() as u64;
            if len != length.value {
                return Err(FacetError::length(format!(
                    "value length {} does not equal required length {}",
                    len, length.value
                )));
            }
        }

        // Check minLength facet
        if let Some(ref min_length) = self.min_length {
            let len = check_value.chars().count() as u64;
            if len < min_length.value {
                return Err(FacetError::MinLengthViolation {
                    actual: len,
                    min: min_length.value,
                });
            }
        }

        // Check maxLength facet
        if let Some(ref max_length) = self.max_length {
            let len = check_value.chars().count() as u64;
            if len > max_length.value {
                return Err(FacetError::MaxLengthViolation {
                    actual: len,
                    max: max_length.value,
                });
            }
        }

        // Check all pattern steps (each step's alternatives are OR'd; all
        // steps must match).
        self.check_patterns(check_value)?;

        // Check enumeration
        if let Some(ref enumeration) = self.enumeration {
            if !enumeration.values.contains(check_value) {
                return Err(FacetError::enumeration(check_value));
            }
        }

        Ok(())
    }

    /// Validate only pattern and enumeration facets on a string value.
    /// Used for list types where length facets are checked separately as item count.
    pub fn validate_string_patterns_enums(&self, value: &str) -> FacetResult<()> {
        let normalized = match &self.whitespace {
            Some(ws) => normalize_whitespace(value, ws.value),
            None => Cow::Borrowed(value),
        };
        let check_value: &str = &normalized;

        self.check_patterns(check_value)?;

        if let Some(ref enumeration) = self.enumeration {
            if !enumeration.values.contains(check_value) {
                return Err(FacetError::enumeration(check_value));
            }
        }

        Ok(())
    }

    /// Validate only pattern facets (no enumeration, no length).
    /// Used when enumeration must be checked in value space rather than lexically.
    pub fn validate_patterns_only(&self, value: &str) -> FacetResult<()> {
        let normalized = match &self.whitespace {
            Some(ws) => normalize_whitespace(value, ws.value),
            None => Cow::Borrowed(value),
        };
        self.check_patterns(&normalized)
    }

    /// Validate enumeration in value space using a caller-supplied match predicate.
    /// `is_match(enum_str)` returns true if the instance value equals the given
    /// enumeration lexical value. `display` is used in the error message on failure.
    pub fn validate_enum_value_space(
        &self,
        is_match: impl Fn(&str) -> bool,
        display: &str,
    ) -> FacetResult<()> {
        if let Some(ref enumeration) = self.enumeration {
            if !enumeration.values.iter().any(|s| is_match(s)) {
                return Err(FacetError::enumeration(display));
            }
        }
        Ok(())
    }

    /// Validate a decimal value against numeric facets
    ///
    /// `totalDigits` and `fractionDigits` follow their definitions on the value
    /// (Datatypes 1.1 §4.3.11.3 / §4.3.12.3): the value is `i / 10^n` with
    /// `|i| < 10^totalDigits` and `0 ≤ n ≤ totalDigits`, resp.
    /// `0 ≤ n ≤ fractionDigits` — so `0.05` needs two total digits. Bound
    /// facets compare exactly, including bounds that are themselves beyond
    /// what `rust_decimal::Decimal` holds.
    pub fn validate_decimal(&self, value: &rust_decimal::Decimal) -> FacetResult<()> {
        self.check_decimal_facets(
            count_total_digits(value) as usize,
            count_fraction_digits(value) as usize,
            |bound| compare_decimal_with_bound(value, bound),
            || value.to_string(),
        )
    }

    /// [`FacetSet::validate_decimal`] for a value of any size and precision.
    pub(crate) fn validate_exact_decimal(
        &self,
        value: &super::exact_decimal::ExactDecimal,
    ) -> FacetResult<()> {
        self.check_decimal_facets(
            value.total_digits(),
            value.fraction_digits(),
            |bound| {
                let bound = super::exact_decimal::ExactDecimal::parse(&normalize_whitespace(
                    bound,
                    WhitespaceMode::Collapse,
                ))?;
                Some(value.cmp(&bound))
            },
            || value.to_string(),
        )
    }

    /// The value-based facets of an `xs:decimal`-family typed value — digits
    /// and bounds on the exact value, then `pattern` on the lexical form and
    /// `enumeration` by value (Datatypes 1.1 §4.3.5.4: "A value in a ·value
    /// space· is facet-valid with respect to ·enumeration· if and only if the
    /// value is equal or identical to one of the values specified in
    /// {value}"; §3.3.3: "the number 2.0 is not distinct from the number
    /// 2.00").
    ///
    /// `value` is the typed value a decimal-family validator produced from
    /// `normalized`, its whitespace-collapsed lexical form.
    pub(crate) fn validate_decimal_family(
        &self,
        value: &super::value::XmlValue,
        normalized: &str,
    ) -> FacetResult<()> {
        use super::exact_decimal::ExactDecimal;
        use super::value::{XmlAtomicValue, XmlValueKind};
        match &value.value {
            XmlValueKind::Atomic(XmlAtomicValue::Decimal(d)) => self.validate_decimal(d)?,
            XmlValueKind::Atomic(XmlAtomicValue::Integer(i)) => match integer_as_decimal(i) {
                Some(d) => self.validate_decimal(&d)?,
                None => self.validate_exact_decimal(&ExactDecimal::from_bigint(i))?,
            },
            _ => {
                if let Some(exact) = ExactDecimal::from_value(value) {
                    self.validate_exact_decimal(&exact)?;
                }
            }
        }
        self.validate_patterns_only(normalized)?;
        self.validate_decimal_enumeration(value, normalized)
    }

    /// `enumeration` for an `xs:decimal`-family value, compared by value. A
    /// lexically identical member is accepted without parsing anything.
    fn validate_decimal_enumeration(
        &self,
        value: &super::value::XmlValue,
        normalized: &str,
    ) -> FacetResult<()> {
        use super::exact_decimal::ExactDecimal;
        let Some(ref enumeration) = self.enumeration else {
            return Ok(());
        };
        if enumeration.values.contains(normalized) {
            return Ok(());
        }
        let found = ExactDecimal::from_value(value).is_some_and(|exact| {
            enumeration.values.iter().any(|member| {
                ExactDecimal::parse(&normalize_whitespace(member, WhitespaceMode::Collapse))
                    .is_some_and(|m| m == exact)
            })
        });
        if found {
            Ok(())
        } else {
            Err(FacetError::enumeration(normalized))
        }
    }

    /// The digit and bound facets, given the value's digit counts, a
    /// comparison of the value with a bound literal (`None` when the literal
    /// does not parse — such a bound was rejected when the schema compiled),
    /// and the value's display form for messages.
    fn check_decimal_facets(
        &self,
        total_digits: usize,
        fraction_digits: usize,
        compare_with: impl Fn(&str) -> Option<std::cmp::Ordering>,
        display: impl Fn() -> String,
    ) -> FacetResult<()> {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let saturate = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);

        if let Some(ref td) = self.total_digits {
            if total_digits > td.value as usize {
                return Err(FacetError::TotalDigitsViolation {
                    actual: saturate(total_digits),
                    max: td.value,
                });
            }
        }

        if let Some(ref fd) = self.fraction_digits {
            if fraction_digits > fd.value as usize {
                return Err(FacetError::FractionDigitsViolation {
                    actual: saturate(fraction_digits),
                    max: fd.value,
                });
            }
        }

        if let Some(ref min) = self.min_inclusive {
            if compare_with(&min.value) == Some(Less) {
                return Err(FacetError::MinInclusiveViolation {
                    value: display(),
                    min: min.value.clone(),
                });
            }
        }

        if let Some(ref max) = self.max_inclusive {
            if compare_with(&max.value) == Some(Greater) {
                return Err(FacetError::MaxInclusiveViolation {
                    value: display(),
                    max: max.value.clone(),
                });
            }
        }

        if let Some(ref min) = self.min_exclusive {
            if matches!(compare_with(&min.value), Some(Less | Equal)) {
                return Err(FacetError::MinExclusiveViolation {
                    value: display(),
                    min: min.value.clone(),
                });
            }
        }

        if let Some(ref max) = self.max_exclusive {
            if matches!(compare_with(&max.value), Some(Greater | Equal)) {
                return Err(FacetError::MaxExclusiveViolation {
                    value: display(),
                    max: max.value.clone(),
                });
            }
        }

        Ok(())
    }

    /// Validate a float value against numeric bounds facets
    ///
    /// Datatypes 1.1 §3.3.4.1: "NaN is ·incomparable· with any value in the
    /// ·value space· including itself", and a bound holds only for a value
    /// ordered as it requires "according to the datatype's order relation"
    /// (§4.3.7.3 – §4.3.10.3) — so NaN satisfies no bound, and a NaN bound no
    /// value. (In XSD 1.0 NaN equals itself and satisfies an inclusive NaN
    /// bound; the instance validator applies that rule in XSD 1.0 mode.) The
    /// bound is read with the `xs:float` lexical mapping, after the whitespace
    /// collapse its datatype prescribes.
    pub fn validate_float(&self, value: f32) -> FacetResult<()> {
        self.check_ordered_bounds(
            |bound| {
                let bound = super::validators::parse_float(&normalize_whitespace(
                    bound,
                    WhitespaceMode::Collapse,
                ))
                .ok()?;
                Some(value.partial_cmp(&bound))
            },
            || format_float_for_error(value),
        )
    }

    /// Validate a double value against numeric bounds facets
    ///
    /// As [`FacetSet::validate_float`], with the `xs:double` order (Datatypes
    /// 1.1 §3.3.5.1).
    pub fn validate_double(&self, value: f64) -> FacetResult<()> {
        self.check_ordered_bounds(
            |bound| {
                let bound = super::validators::parse_double(&normalize_whitespace(
                    bound,
                    WhitespaceMode::Collapse,
                ))
                .ok()?;
                Some(value.partial_cmp(&bound))
            },
            || format_double_for_error(value),
        )
    }

    /// The four bounding facets of a datatype with a partial order, given the
    /// order of the value relative to a bound literal — `None` when the
    /// literal does not parse (such a bound was rejected when the schema
    /// compiled), `Some(None)` when the two are ·incomparable·, which no bound
    /// admits — and the value's display form for messages.
    fn check_ordered_bounds(
        &self,
        compare_with: impl Fn(&str) -> Option<Option<std::cmp::Ordering>>,
        display: impl Fn() -> String,
    ) -> FacetResult<()> {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let holds = |bound: &str, admitted: &[std::cmp::Ordering]| match compare_with(bound) {
            None => true,
            Some(order) => order.is_some_and(|order| admitted.contains(&order)),
        };

        if let Some(ref min) = self.min_inclusive {
            if !holds(&min.value, &[Greater, Equal]) {
                return Err(FacetError::MinInclusiveViolation {
                    value: display(),
                    min: min.value.clone(),
                });
            }
        }

        if let Some(ref max) = self.max_inclusive {
            if !holds(&max.value, &[Less, Equal]) {
                return Err(FacetError::MaxInclusiveViolation {
                    value: display(),
                    max: max.value.clone(),
                });
            }
        }

        if let Some(ref min) = self.min_exclusive {
            if !holds(&min.value, &[Greater]) {
                return Err(FacetError::MinExclusiveViolation {
                    value: display(),
                    min: min.value.clone(),
                });
            }
        }

        if let Some(ref max) = self.max_exclusive {
            if !holds(&max.value, &[Less]) {
                return Err(FacetError::MaxExclusiveViolation {
                    value: display(),
                    max: max.value.clone(),
                });
            }
        }

        Ok(())
    }

    /// Validate explicitTimezone constraint (XSD 1.1)
    ///
    /// # Arguments
    /// * `has_timezone` - Whether the value has a timezone specified
    pub fn validate_explicit_timezone(&self, has_timezone: bool) -> FacetResult<()> {
        if let Some(ref etz) = self.explicit_timezone {
            match etz.value {
                ExplicitTimezone::Required if !has_timezone => {
                    return Err(FacetError::ExplicitTimezoneViolation {
                        message: "timezone is required but not present".to_string(),
                    });
                }
                ExplicitTimezone::Prohibited if has_timezone => {
                    return Err(FacetError::ExplicitTimezoneViolation {
                        message: "timezone is prohibited but present".to_string(),
                    });
                }
                ExplicitTimezone::Optional
                | ExplicitTimezone::Required
                | ExplicitTimezone::Prohibited => {
                    // Valid
                }
            }
        }
        Ok(())
    }

    /// Validate a binary value (hex or base64) against length facets
    pub fn validate_binary_length(&self, byte_count: u64) -> FacetResult<()> {
        // For binary types, length is measured in octets
        if let Some(ref length) = self.length {
            if byte_count != length.value {
                return Err(FacetError::length(format!(
                    "binary length {} does not equal required length {}",
                    byte_count, length.value
                )));
            }
        }

        if let Some(ref min_length) = self.min_length {
            if byte_count < min_length.value {
                return Err(FacetError::MinLengthViolation {
                    actual: byte_count,
                    min: min_length.value,
                });
            }
        }

        if let Some(ref max_length) = self.max_length {
            if byte_count > max_length.value {
                return Err(FacetError::MaxLengthViolation {
                    actual: byte_count,
                    max: max_length.value,
                });
            }
        }

        Ok(())
    }

    /// Validate a list value against length facets (item count)
    pub fn validate_list_length(&self, item_count: u64) -> FacetResult<()> {
        // For list types, length is measured in number of items
        if let Some(ref length) = self.length {
            if item_count != length.value {
                return Err(FacetError::length(format!(
                    "list length {} does not equal required length {}",
                    item_count, length.value
                )));
            }
        }

        if let Some(ref min_length) = self.min_length {
            if item_count < min_length.value {
                return Err(FacetError::MinLengthViolation {
                    actual: item_count,
                    min: min_length.value,
                });
            }
        }

        if let Some(ref max_length) = self.max_length {
            if item_count > max_length.value {
                return Err(FacetError::MaxLengthViolation {
                    actual: item_count,
                    max: max_length.value,
                });
            }
        }

        Ok(())
    }
}

/// Check if derived whitespace mode is more restrictive than base
fn is_whitespace_more_restrictive(derived: WhitespaceMode, base: WhitespaceMode) -> bool {
    use WhitespaceMode::*;
    match (base, derived) {
        // Same is always OK
        (Preserve, Preserve) | (Replace, Replace) | (Collapse, Collapse) => true,
        // preserve -> replace -> collapse is more restrictive
        (Preserve, Replace) | (Preserve, Collapse) | (Replace, Collapse) => true,
        // Going the other way is less restrictive
        _ => false,
    }
}

/// A facet literal as a value of `datatype`, through its registered
/// validator's lexical mapping; `None` when the datatype has no validator or
/// the literal is not one of its lexical forms.
fn literal_value(datatype: XmlTypeCode, literal: &str) -> Option<super::value::XmlValue> {
    super::validators::VALIDATOR_REGISTRY
        .get_by_code(datatype)?
        .validate(literal)
        .ok()
}

/// The order of two bound literals of `datatype` in its value space, or
/// `None` when either is not a literal of the datatype (such a bound is
/// rejected on its own), the datatype has no order here, or the two are
/// ·incomparable· (`NaN`; date/time values with and without a time zone
/// offset; durations such as `P1M` and `P30D`).
///
/// Exact comparison is right only for `xs:decimal`: two `xs:double`
/// literals that differ beyond double precision, such as `0.1` and
/// `0.10000000000000000001`, denote the same value.
pub(crate) fn literal_order(datatype: XmlTypeCode, a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use super::exact_decimal::ExactDecimal;
    use super::validators::{parse_double, parse_float};
    use super::PrimitiveTypeCode;
    let collapse = |s: &str| normalize_whitespace(s, WhitespaceMode::Collapse).into_owned();
    match PrimitiveTypeCode::from_type_code(datatype)? {
        PrimitiveTypeCode::Decimal => {
            Some(ExactDecimal::parse(&collapse(a))?.cmp(&ExactDecimal::parse(&collapse(b))?))
        }
        PrimitiveTypeCode::Float => parse_float(&collapse(a))
            .ok()?
            .partial_cmp(&parse_float(&collapse(b)).ok()?),
        PrimitiveTypeCode::Double => parse_double(&collapse(a))
            .ok()?
            .partial_cmp(&parse_double(&collapse(b)).ok()?),
        _ => {
            use super::value::XmlValueKind::Atomic;
            use super::value_order::{literal_cmp_in, negative_year};
            match (
                literal_value(datatype, a)?.value,
                literal_value(datatype, b)?.value,
            ) {
                // XSD 1.0 and 1.1 number the years before year 1 differently
                // (1.0 has no year 0000); this method does not know the
                // version, so such a pair is not ordered here.
                (Atomic(x), Atomic(y)) if negative_year(&x) != negative_year(&y) => None,
                (Atomic(x), Atomic(y)) => literal_cmp_in(false, &x, a, &y, b),
                _ => None,
            }
        }
    }
}

/// Whether two facet literals denote the same value of `datatype`: equal or
/// identical (Datatypes 1.1 §2.2.1 – §2.2.2) as the instance comparisons
/// decide it — decimals exactly, `NaN` identical to itself and `0` equal to
/// `-0`, date/time values on the time line, durations by months and
/// seconds, strings by their characters, lists member for member. Literals
/// the datatype does not read (or every literal, without a validator) are
/// equal only when they are written alike.
pub(crate) fn literal_values_equal(datatype: XmlTypeCode, a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (literal_value(datatype, a), literal_value(datatype, b)) {
        (Some(x), Some(y)) => {
            crate::validation::identity::literal_values_equal_or_identical(&x, a, &y, b)
        }
        _ => false,
    }
}

/// Apply whitespace normalization to a string.
///
/// Returns [`Cow::Borrowed`] when the input is already in the target form (the
/// common case for clean instance values) — no allocation — and
/// [`Cow::Owned`] only when a change is actually required. The produced
/// string is byte-identical to the previous always-allocating implementation.
pub fn normalize_whitespace(s: &str, mode: WhitespaceMode) -> Cow<'_, str> {
    match mode {
        WhitespaceMode::Preserve => Cow::Borrowed(s),
        WhitespaceMode::Replace => replace_whitespace(s),
        WhitespaceMode::Collapse => collapse_whitespace(s),
    }
}

/// `true` for exactly the four XSD whitespace characters #x20 #x9 #xD #xA.
///
/// Deliberately byte-level and *not* [`char::is_whitespace`] / [`str::trim`] /
/// [`str::split_whitespace`]: those match Unicode whitespace (U+00A0, U+2028, …),
/// which XSD `whiteSpace` must leave untouched. All four are ASCII, so trimming
/// or splitting on them never lands inside a multi-byte UTF-8 sequence.
#[inline]
fn is_xsd_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

/// `replace` whitespace normalization: each #x9/#xD/#xA becomes #x20 (a #x20 is
/// already itself). Borrows when there is nothing to replace; otherwise allocates
/// once and bulk-copies the unchanged runs around each replaced byte.
fn replace_whitespace(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    if !bytes.iter().any(|&b| matches!(b, b'\t' | b'\r' | b'\n')) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chunk_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(b, b'\t' | b'\r' | b'\n') {
            out.push_str(&s[chunk_start..i]); // unchanged run before this byte
            out.push(' ');
            chunk_start = i + 1;
        }
    }
    out.push_str(&s[chunk_start..]); // trailing unchanged run
    Cow::Owned(out)
}

/// `collapse` whitespace normalization: `replace`, then squeeze each run of
/// whitespace to a single #x20, then strip leading/trailing whitespace.
///
/// Two no-alloc fast paths cover the common shapes:
/// * all-whitespace (or empty) → the empty string;
/// * a trimmed core that needs no internal rewriting (no #x9/#xD/#xA and no
///   double whitespace) → a borrowed sub-slice, so `" 123 "` / `"\nfoo\t"`
///   borrow instead of allocating.
///
/// Only genuinely dirty input (internal tabs/newlines or multi-space runs)
/// allocates, and then the rewrite copies whole words with `push_str` rather
/// than one `char` at a time.
fn collapse_whitespace(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    // Trim XSD whitespace from both ends. The cut points are ASCII bytes, hence
    // always char boundaries.
    let Some(start) = bytes.iter().position(|&b| !is_xsd_ws(b)) else {
        return Cow::Borrowed(""); // all whitespace (or empty) collapses to ""
    };
    let end = bytes.iter().rposition(|&b| !is_xsd_ws(b)).unwrap() + 1;
    let mid = &s[start..end];

    // Borrow the trimmed core unless it needs internal rewriting: any #x9/#xD/#xA
    // (which `replace` would turn into a space) or any run of two+ whitespace
    // (which `collapse` would squeeze). The core has no leading/trailing ws.
    let mut prev_ws = false;
    let mut needs_rewrite = false;
    for &b in mid.as_bytes() {
        match b {
            b'\t' | b'\r' | b'\n' => {
                needs_rewrite = true;
                break;
            }
            b' ' => {
                if prev_ws {
                    needs_rewrite = true;
                    break;
                }
                prev_ws = true;
            }
            _ => prev_ws = false,
        }
    }
    if !needs_rewrite {
        return Cow::Borrowed(mid);
    }

    // Rewrite: emit each non-whitespace word, separated by a single space. `mid`
    // is already trimmed, so there is exactly one space between words and none at
    // the ends — byte-identical to the previous char-by-char implementation.
    let mut out = String::with_capacity(mid.len());
    let mbytes = mid.as_bytes();
    let mut i = 0;
    let mut first_word = true;
    while i < mbytes.len() {
        while i < mbytes.len() && is_xsd_ws(mbytes[i]) {
            i += 1; // skip a whitespace run
        }
        if i >= mbytes.len() {
            break;
        }
        if !first_word {
            out.push(' ');
        }
        first_word = false;
        let word_start = i;
        while i < mbytes.len() && !is_xsd_ws(mbytes[i]) {
            i += 1;
        }
        out.push_str(&mid[word_start..i]);
    }
    Cow::Owned(out)
}

/// The `totalDigits` a decimal value needs (Datatypes 1.1 §4.3.11.3): the
/// value is `i / 10^n` with `|i| < 10^t` and `0 ≤ n ≤ t`. With the value
/// normalized to the mantissa `i` and scale `n` (no trailing zeros), that is
/// the larger of the digit count of `i` and `n` — `0.12345 = 12345 × 10^-5`
/// needs five digits (the leading `0` of the lexical form is not one of them),
/// and `0.05 = 5 × 10^-2` needs two.
fn count_total_digits(value: &rust_decimal::Decimal) -> u32 {
    let normalized = value.normalize();
    let mut m = normalized.mantissa().unsigned_abs();
    let mut count = 0u32;
    while m > 0 {
        count += 1;
        m /= 10;
    }
    count.max(normalized.scale()).max(1)
}

/// The `fractionDigits` a decimal value needs (Datatypes 1.1 §4.3.12.3): the
/// scale of the normalized value.
fn count_fraction_digits(value: &rust_decimal::Decimal) -> u32 {
    value.normalize().scale()
}

/// An `xs:integer` value as a `rust_decimal::Decimal`, when it fits.
fn integer_as_decimal(value: &num_bigint::BigInt) -> Option<rust_decimal::Decimal> {
    match i64::try_from(value) {
        Ok(small) => Some(rust_decimal::Decimal::from(small)),
        Err(_) => value.to_string().parse().ok(),
    }
}

/// The order of a decimal value relative to a bound literal, exactly — also
/// when the bound is beyond what `rust_decimal::Decimal` holds. `None` when
/// the literal is not an `xs:decimal` lexical form.
fn compare_decimal_with_bound(
    value: &rust_decimal::Decimal,
    bound: &str,
) -> Option<std::cmp::Ordering> {
    if let Ok(bound) = rust_decimal::Decimal::from_str_exact(bound) {
        return Some(value.cmp(&bound));
    }
    let bound = super::exact_decimal::ExactDecimal::parse(&normalize_whitespace(
        bound,
        WhitespaceMode::Collapse,
    ))?;
    Some(super::exact_decimal::ExactDecimal::from_decimal(value).cmp(&bound))
}

/// Format a float value for error messages (XSD canonical form)
fn format_float_for_error(v: f32) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v.is_sign_positive() {
            "INF".to_string()
        } else {
            "-INF".to_string()
        }
    } else {
        v.to_string()
    }
}

/// Format a double value for error messages (XSD canonical form)
fn format_double_for_error(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v.is_sign_positive() {
            "INF".to_string()
        } else {
            "-INF".to_string()
        }
    } else {
        v.to_string()
    }
}

/// Facet applicability for built-in types
///
/// Defines which facets can be applied to which primitive types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacetApplicability {
    /// Facet is not applicable to this type
    NotApplicable,
    /// Facet is applicable to this type
    Applicable,
    /// Facet is required for this type (e.g., whitespace for string)
    Required,
}

/// Facet kind enumeration for checking applicability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacetKind {
    Length,
    MinLength,
    MaxLength,
    Pattern,
    Enumeration,
    Whitespace,
    MinInclusive,
    MaxInclusive,
    MinExclusive,
    MaxExclusive,
    TotalDigits,
    FractionDigits,
    /// XSD 1.1
    ExplicitTimezone,
    /// XSD 1.1
    Assertion,
}

impl FacetKind {
    /// Parse facet kind from name
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "length" => Some(Self::Length),
            "minLength" => Some(Self::MinLength),
            "maxLength" => Some(Self::MaxLength),
            "pattern" => Some(Self::Pattern),
            "enumeration" => Some(Self::Enumeration),
            "whiteSpace" => Some(Self::Whitespace),
            "minInclusive" => Some(Self::MinInclusive),
            "maxInclusive" => Some(Self::MaxInclusive),
            "minExclusive" => Some(Self::MinExclusive),
            "maxExclusive" => Some(Self::MaxExclusive),
            "totalDigits" => Some(Self::TotalDigits),
            "fractionDigits" => Some(Self::FractionDigits),
            "explicitTimezone" => Some(Self::ExplicitTimezone),
            "assertion" => Some(Self::Assertion),
            _ => None,
        }
    }

    /// Get the name of this facet kind
    pub fn name(&self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::MinLength => "minLength",
            Self::MaxLength => "maxLength",
            Self::Pattern => "pattern",
            Self::Enumeration => "enumeration",
            Self::Whitespace => "whiteSpace",
            Self::MinInclusive => "minInclusive",
            Self::MaxInclusive => "maxInclusive",
            Self::MinExclusive => "minExclusive",
            Self::MaxExclusive => "maxExclusive",
            Self::TotalDigits => "totalDigits",
            Self::FractionDigits => "fractionDigits",
            Self::ExplicitTimezone => "explicitTimezone",
            Self::Assertion => "assertion",
        }
    }
}

/// Check if a facet is applicable to a type (using XmlTypeCode)
pub fn facet_applicable_for_type(facet: FacetKind, type_code: XmlTypeCode) -> FacetApplicability {
    use FacetApplicability::*;
    use FacetKind::*;
    use XmlTypeCode::*;

    match facet {
        // Length facets apply to string, binary, list, and URI types
        Length | MinLength | MaxLength => match type_code {
            String | NormalizedString | Token | Language | NmToken | Name | NCName | Id | IdRef
            | Entity | HexBinary | Base64Binary | AnyUri | QName | Notation | NmTokens | IdRefs
            | Entities => Applicable,
            _ => NotApplicable,
        },

        // Pattern and enumeration apply to all atomic types
        Pattern | Enumeration => {
            if type_code.is_atomic() || type_code == AnySimpleType || type_code == AnyAtomicType {
                Applicable
            } else {
                NotApplicable
            }
        }

        // Whitespace is required for string, applicable to all string-derived types
        Whitespace => match type_code {
            String => Required,
            NormalizedString | Token | Language | NmToken | Name | NCName | Id | IdRef | Entity => {
                Applicable
            }
            // All other atomic types can have whitespace
            _ if type_code.is_atomic() => Applicable,
            _ => NotApplicable,
        },

        // Bound facets apply to ordered types (numeric, date/time)
        MinInclusive | MaxInclusive | MinExclusive | MaxExclusive => match type_code {
            // Decimal hierarchy
            Decimal | Integer | NonPositiveInteger | NegativeInteger | NonNegativeInteger
            | PositiveInteger | Long | Int | Short | Byte | UnsignedLong | UnsignedInt
            | UnsignedShort | UnsignedByte => Applicable,
            // Float/Double
            Float | Double => Applicable,
            // Date/time types (all have total ordering)
            Duration | DateTime | Time | Date | GYearMonth | GYear | GMonthDay | GDay | GMonth
            | YearMonthDuration | DayTimeDuration | DateTimeStamp => Applicable,
            _ => NotApplicable,
        },

        // Digit facets apply only to decimal types
        TotalDigits => match type_code {
            Decimal | Integer | NonPositiveInteger | NegativeInteger | NonNegativeInteger
            | PositiveInteger | Long | Int | Short | Byte | UnsignedLong | UnsignedInt
            | UnsignedShort | UnsignedByte => Applicable,
            _ => NotApplicable,
        },

        FractionDigits => match type_code {
            Decimal => Applicable,
            // Integer types have fractionDigits implicitly 0
            Integer | NonPositiveInteger | NegativeInteger | NonNegativeInteger
            | PositiveInteger | Long | Int | Short | Byte | UnsignedLong | UnsignedInt
            | UnsignedShort | UnsignedByte => Applicable,
            _ => NotApplicable,
        },

        // XSD 1.1: explicitTimezone applies to date/time types with optional timezone
        ExplicitTimezone => match type_code {
            DateTime | Time | Date | GYearMonth | GYear | GMonthDay | GDay | GMonth
            | DateTimeStamp => Applicable,
            _ => NotApplicable,
        },

        // XSD 1.1: assertion applies to all types
        Assertion => Applicable,
    }
}

/// Check if a facet is applicable to a built-in type (by name)
///
/// This is a convenience wrapper around `facet_applicable_for_type` that
/// takes string names for compatibility.
pub fn facet_applicable(type_name: &str, facet_name: &str) -> FacetApplicability {
    let facet = match FacetKind::from_name(facet_name) {
        Some(f) => f,
        None => return FacetApplicability::NotApplicable,
    };

    let type_code = match XmlTypeCode::from_local_name(type_name) {
        Some(tc) => tc,
        None => return FacetApplicability::NotApplicable,
    };

    facet_applicable_for_type(facet, type_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    // =========================================================================
    // Basic FacetSet tests
    // =========================================================================

    #[test]
    fn test_facet_set_empty() {
        let facets = FacetSet::new();
        assert!(facets.is_empty());
    }

    #[test]
    fn test_facet_set_length() {
        let mut facets = FacetSet::new();
        facets.set_length(10, FacetFixed::Default, None);

        assert!(!facets.is_empty());
        assert_eq!(facets.length.as_ref().unwrap().value, 10);
    }

    #[test]
    fn test_facet_set_patterns() {
        let mut facets = FacetSet::new();
        facets
            .add_pattern(
                "[a-z]+".to_string(),
                None,
                XsdVersion::V1_1,
                RegexCompat::Strict,
            )
            .unwrap();
        facets
            .add_pattern(
                "[0-9]+".to_string(),
                None,
                XsdVersion::V1_1,
                RegexCompat::Strict,
            )
            .unwrap();

        // Two adds within one FacetSet share the same derivation step (OR'd).
        assert_eq!(facets.patterns.len(), 1);
        assert_eq!(facets.patterns[0].len(), 2);
    }

    #[test]
    fn test_facet_set_enumeration() {
        let mut facets = FacetSet::new();
        facets.add_enumeration("red".to_string(), None);
        facets.add_enumeration("green".to_string(), None);
        facets.add_enumeration("blue".to_string(), None);

        let enum_facet = facets.enumeration.as_ref().unwrap();
        assert_eq!(enum_facet.values.len(), 3);
        assert!(enum_facet.values.contains("red"));
    }

    #[test]
    fn test_facet_inheritance() {
        let mut base = FacetSet::new();
        base.set_min_length(5, FacetFixed::Fixed, None);
        base.set_max_length(100, FacetFixed::Default, None);
        base.add_pattern(
            "[a-z]+".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        )
        .unwrap();

        let mut derived = FacetSet::new();
        derived.set_max_length(50, FacetFixed::Default, None); // Override

        derived.inherit_from(&base);

        // minLength inherited
        assert_eq!(derived.min_length.as_ref().unwrap().value, 5);
        // maxLength not inherited (was overridden)
        assert_eq!(derived.max_length.as_ref().unwrap().value, 50);
        // Base step inherited as a separate step entry.
        assert_eq!(derived.patterns.len(), 1);
        assert_eq!(derived.patterns[0].len(), 1);
    }

    /// Structures 1.1 §3.16.6.4 clause 3: "The {facets} of R are the {facets}
    /// of B ·overlaid· with S" — a restriction without its own `enumeration`
    /// keeps the base's (with the namespace bindings of its QName values);
    /// one with its own replaces it.
    #[test]
    fn test_inherit_from_keeps_the_base_enumeration() {
        let mut base = FacetSet::new();
        base.add_enumeration("red".to_string(), None);
        base.add_enumeration_with_ns(
            "my:a".to_string(),
            Some(NamespaceContextSnapshot::default()),
            None,
        );

        let mut plain = FacetSet::new();
        plain.set_max_length(4, FacetFixed::Default, None);
        plain.inherit_from(&base);
        let inherited = plain.enumeration.as_ref().unwrap();
        assert_eq!(inherited.values.len(), 2);
        assert!(inherited.values.contains("red"));
        assert_eq!(inherited.qname_contexts.len(), 1);
        assert!(plain.validate_string("blue").is_err());
        assert!(plain.validate_string("red").is_ok());

        let mut own = FacetSet::new();
        own.add_enumeration("red".to_string(), None);
        own.inherit_from(&base);
        let own_values = &own.enumeration.as_ref().unwrap().values;
        assert_eq!(own_values.len(), 1);
        assert!(own.validate_string("my:a").is_err());
    }

    /// Datatypes 1.1 §3.3.4.1 / §3.3.5.1: "NaN is ·incomparable· with any
    /// value in the ·value space· including itself", and "If any other value
    /// is used for a bounding facet, NaN will be excluded from the resulting
    /// restricted ·value space·"; "when NaN is used as a facet value for a
    /// bounding facet, since no float values are ·comparable· with it, the
    /// result is a ·value space· that is empty". (The facet set implements
    /// Datatypes 1.1; XSD 1.0's NaN, equal to itself, satisfies an inclusive
    /// NaN bound — `validate_atomic_type` takes care of that, tested in
    /// `tests/facet_soundness.rs`.)
    #[test]
    fn test_nan_is_outside_every_bound() {
        type Setter = fn(&mut FacetSet, String);
        let setters: [(&str, Setter); 4] = [
            ("minInclusive", |f, v| {
                f.set_min_inclusive(v, FacetFixed::Default, None)
            }),
            ("maxInclusive", |f, v| {
                f.set_max_inclusive(v, FacetFixed::Default, None)
            }),
            ("minExclusive", |f, v| {
                f.set_min_exclusive(v, FacetFixed::Default, None)
            }),
            ("maxExclusive", |f, v| {
                f.set_max_exclusive(v, FacetFixed::Default, None)
            }),
        ];
        for (name, set) in setters {
            for bound in ["0", "INF", "-INF", "1.5E3"] {
                let mut facets = FacetSet::new();
                set(&mut facets, bound.to_string());
                assert!(
                    facets.validate_double(f64::NAN).is_err(),
                    "double NaN vs {name} {bound}"
                );
                assert!(
                    facets.validate_float(f32::NAN).is_err(),
                    "float NaN vs {name} {bound}"
                );
            }
            let mut facets = FacetSet::new();
            set(&mut facets, "NaN".to_string());
            for value in [0.0, -1.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
                assert!(
                    facets.validate_double(value).is_err(),
                    "double {value} vs {name} NaN"
                );
                assert!(
                    facets.validate_float(value as f32).is_err(),
                    "float {value} vs {name} NaN"
                );
            }
        }
        // Comparable values keep their verdicts.
        let mut pct = FacetSet::new();
        pct.set_min_inclusive("0".to_string(), FacetFixed::Default, None);
        pct.set_max_inclusive("100".to_string(), FacetFixed::Default, None);
        for ok in [0.0, -0.0, 50.0, 100.0] {
            assert!(pct.validate_double(ok).is_ok(), "{ok}");
            assert!(pct.validate_float(ok as f32).is_ok(), "{ok}");
        }
        for bad in [-1.0, 100.5, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(pct.validate_double(bad).is_err(), "{bad}");
            assert!(pct.validate_float(bad as f32).is_err(), "{bad}");
        }
        let mut unbounded_above = FacetSet::new();
        unbounded_above.set_max_inclusive("INF".to_string(), FacetFixed::Default, None);
        assert!(unbounded_above.validate_double(f64::INFINITY).is_ok());
        assert!(unbounded_above.validate_float(f32::INFINITY).is_ok());
    }

    // =========================================================================
    // Facet applicability tests
    // =========================================================================

    #[test]
    fn test_facet_applicability() {
        use FacetApplicability::*;

        // Length facets apply to string types
        assert_eq!(facet_applicable("string", "length"), Applicable);
        assert_eq!(facet_applicable("decimal", "length"), NotApplicable);

        // Numeric facets apply to numeric types
        assert_eq!(facet_applicable("decimal", "minInclusive"), Applicable);
        assert_eq!(facet_applicable("string", "minInclusive"), NotApplicable);

        // Pattern and enumeration apply to all
        assert_eq!(facet_applicable("string", "pattern"), Applicable);
        assert_eq!(facet_applicable("decimal", "pattern"), Applicable);

        // Whitespace is required for string
        assert_eq!(facet_applicable("string", "whiteSpace"), Required);
    }

    #[test]
    fn test_facet_applicability_with_type_code() {
        use FacetApplicability::*;
        use FacetKind::*;
        use XmlTypeCode::*;

        // Length facets
        assert_eq!(facet_applicable_for_type(Length, String), Applicable);
        assert_eq!(facet_applicable_for_type(Length, HexBinary), Applicable);
        assert_eq!(facet_applicable_for_type(Length, Decimal), NotApplicable);

        // Digit facets
        assert_eq!(facet_applicable_for_type(TotalDigits, Decimal), Applicable);
        assert_eq!(facet_applicable_for_type(TotalDigits, Integer), Applicable);
        assert_eq!(facet_applicable_for_type(TotalDigits, Float), NotApplicable);

        // Date/time facets
        assert_eq!(
            facet_applicable_for_type(ExplicitTimezone, DateTime),
            Applicable
        );
        assert_eq!(
            facet_applicable_for_type(ExplicitTimezone, String),
            NotApplicable
        );
    }

    // =========================================================================
    // Pattern tests
    // =========================================================================

    #[test]
    fn test_pattern_matching() {
        let pattern = PatternFacet::new(
            "[a-z]+".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        )
        .unwrap();
        assert!(pattern.matches("hello"));
        assert!(!pattern.matches("HELLO"));
        assert!(!pattern.matches("hello123"));
    }

    #[test]
    fn test_pattern_cache_reuse_and_clear() {
        // Compile the same category-using pattern twice: the second compile
        // should be served from the regex cache (xsd11) yet behave identically.
        let mk = || {
            PatternFacet::new(
                r"\d{3}-\d{4}".to_string(),
                None,
                XsdVersion::V1_1,
                RegexCompat::Strict,
            )
            .unwrap()
        };
        let a = mk();
        let b = mk(); // cache hit under xsd11; full recompile otherwise
        for p in [&a, &b] {
            assert!(p.matches("123-4567"));
            assert!(!p.matches("12-4567"));
            assert!(!p.matches("123-456"));
        }
        // Clearing must not corrupt anything: a fresh compile still works.
        crate::clear_pattern_cache();
        let c = mk();
        assert!(c.matches("123-4567"));
        assert!(!c.matches("xx-yyyy"));
    }

    #[test]
    fn test_pattern_xsd_anchoring() {
        // XSD patterns are implicitly anchored
        let pattern = PatternFacet::new(
            "abc".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        )
        .unwrap();
        assert!(pattern.matches("abc"));
        assert!(!pattern.matches("xabc"));
        assert!(!pattern.matches("abcx"));
    }

    #[test]
    fn test_pattern_xsd_name_chars() {
        // Test \i (initial name char) and \c (name char)
        let pattern = PatternFacet::new(
            r"\i\c*".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        )
        .unwrap();
        assert!(pattern.matches("foo"));
        assert!(pattern.matches("_bar"));
        assert!(pattern.matches("x123"));
        assert!(!pattern.matches("123"));
    }

    #[test]
    fn test_invalid_pattern() {
        let result = PatternFacet::new(
            "[invalid".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_pattern_compiles_independently_of_xpath_dialect_gate() {
        // Pattern facets compile through the regex backend directly, in its
        // XSD dialect (or, under `LenientMs`, its XPath dialect), and match
        // whole values with `is_full_match`. The XPath functions compile in
        // the XPath 2.0 dialect, which rejects `(?`; nothing of that dialect
        // may leak into facet compilation. A facet whose own value contains a
        // group with an alternation is the shape that once went through a
        // `^(?:…)$` wrapper.
        let pattern = PatternFacet::new(
            "(abc|def)+".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        )
        .unwrap();
        assert!(pattern.matches("abcdef"));
        assert!(!pattern.matches("abcx"));

        // Same under XSD 1.0 and under the lenient grammar gate.
        let pattern = PatternFacet::new(
            "(a|b)*".to_string(),
            None,
            XsdVersion::V1_0,
            RegexCompat::LenientMs,
        )
        .unwrap();
        assert!(pattern.matches("abba"));
        assert!(!pattern.matches("abc"));
    }

    #[cfg(feature = "xsd11")]
    fn facet(value: &str, version: XsdVersion, compat: RegexCompat) -> FacetResult<PatternFacet> {
        PatternFacet::new(value.to_string(), None, version, compat)
    }

    /// XSD 1.1 Part 2 §G.3: "A metacharacter is either '.', '\', '?', '*',
    /// '+', '{', '}', '(', ')', '|', '[', or ']'" and "A normal character is
    /// any XML character that is not a ·metacharacter·"; XSD 1.0 Second
    /// Edition Appendix F: "\[10\] Char ::= \[^.\\?*+()|#x5B#x5D\]". So `^`
    /// and `$` outside a character class stand for themselves, in both
    /// versions; they used to be compiled as anchors.
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_caret_and_dollar_are_ordinary_characters() {
        for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
            let p = facet("a$", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("a$"), "{version:?}");
            assert!(!p.matches("a"), "{version:?}");
            let p = facet("^abc", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("^abc"), "{version:?}");
            assert!(!p.matches("abc"), "{version:?}");
            let p = facet(r"\d+\$?", version, RegexCompat::Strict);
            // `\$` is not an XSD escape: `$` needs none.
            assert!(p.is_err(), "{version:?}");
            let p = facet("x^*y$+", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("x^^y$$"), "{version:?}");
            assert!(p.matches("xy$"), "{version:?}");
            assert!(!p.matches("xy"), "{version:?}");
            // `^` keeps its meaning at the start of a class.
            let p = facet("[^a]b", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("cb"), "{version:?}");
            assert!(!p.matches("ab"), "{version:?}");
            let p = facet("[a^]", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("^"), "{version:?}");
        }
    }

    /// `LenientMs` reads a pattern the way .NET does: `^` and `$` anchor, and
    /// the XPath-only idioms are accepted, in both versions.
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_lenient_patterns_keep_anchors_and_xpath_idioms() {
        for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
            let p = facet("^abc$", version, RegexCompat::LenientMs).unwrap();
            assert!(p.matches("abc"), "{version:?}");
            assert!(!p.matches("^abc$"), "{version:?}");
            let p = facet(
                r"(?:a|b)+?(c)\1(?#comment)",
                version,
                RegexCompat::LenientMs,
            )
            .unwrap();
            assert!(p.matches("abcc"), "{version:?}");
            assert!(!p.matches("abc"), "{version:?}");
            // The same pattern is not an XSD regular expression.
            assert!(facet(r"(?:a|b)+?(c)\1", version, RegexCompat::Strict).is_err());
        }
    }

    /// XSD 1.1 Part 2 §G.4.2.4: "If a string "IsX" matches the non-terminal
    /// IsBlock but X is not a recognized block name, then the expressions
    /// "\p{IsX}" and "\P{IsX}" each denote the set of all characters." IsBlock
    /// is `'Is' [a-zA-Z0-9#x2D]+`; a name outside it is still an error, and
    /// XSD 1.0, which lists "the recognized block names", keeps every unknown
    /// block an error.
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_unknown_block_names_denote_every_character_in_xsd11() {
        let v11 = |value: &str| facet(value, XsdVersion::V1_1, RegexCompat::Strict);
        for value in [
            r"\p{IsNoSuchBlock}",
            r"\P{IsNoSuchBlock}",
            r"[\p{IsNo-Such-9}]",
        ] {
            let p = v11(value).unwrap_or_else(|e| panic!("{value}: {e}"));
            for input in ["a", "\u{10FFFF}", "\n"] {
                assert!(p.matches(input), "{value} on {input:?}");
            }
            assert!(!p.matches("ab"), "{value}");
        }
        // Several unknown blocks, in and outside classes, with known ones.
        let p = v11(r"\p{IsFooBar}+[\P{IsBazQux}\p{IsBasicLatin}]\p{IsGreek}").unwrap();
        assert!(p.matches("xyzα"));
        assert!(!p.matches("xyzq"));
        // The complement of every character is no character.
        let p = v11(r"[^\p{IsNoSuchBlock}]?").unwrap();
        assert!(p.matches(""));
        assert!(!p.matches("a"));
        // Not an IsBlock name, or not a block escape at all: errors.
        for value in [r"\p{IsNo_Such}", r"\p{IsNo Such}", r"\p{NoSuchCategory}"] {
            assert!(v11(value).is_err(), "{value}");
        }
        // XSD 1.0: an unknown block is an error, under both compatibility modes.
        for compat in [RegexCompat::Strict, RegexCompat::LenientMs] {
            assert!(facet(r"\p{IsNoSuchBlock}", XsdVersion::V1_0, compat).is_err());
        }
        // A known block keeps its meaning.
        let p = v11(r"\p{IsBasicLatin}+").unwrap();
        assert!(p.matches("abc"));
        assert!(!p.matches("abγ"));
    }

    /// The facet program is matched against the whole value: every way of
    /// matching is tried, not only the one a search finds first, and a counted
    /// repetition keeps its bounds even when its operand can match the empty
    /// string (XSD 1.1 Part 2 §G.2: S{n,m} denotes "All strings in L(S)^k for
    /// all k such that n <= k <= m").
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_whole_value_matching() {
        for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
            let p = facet("a|ab", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("ab") && p.matches("a") && !p.matches("abc"));
            let p = facet(r"(\d|){2}", version, RegexCompat::Strict).unwrap();
            for ok in ["", "1", "12"] {
                assert!(p.matches(ok), "{version:?} {ok:?}");
            }
            for bad in ["123", "1234"] {
                assert!(!p.matches(bad), "{version:?} {bad:?}");
            }
            let p = facet("([a-z]*)+x", version, RegexCompat::Strict).unwrap();
            assert!(p.matches("abx") && p.matches("x") && !p.matches("abxy"));
        }
    }

    /// A pattern that nests groups and classes more than 64 levels deep is
    /// refused (the engine's limit, a stack guard) instead of aborting the
    /// process; 64 levels compile.
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_nesting_limit_is_an_invalid_pattern() {
        let nested = |depth: usize| format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
        for version in [XsdVersion::V1_0, XsdVersion::V1_1] {
            assert!(facet(&nested(64), version, RegexCompat::Strict)
                .unwrap()
                .matches("a"));
            match facet(&nested(65), version, RegexCompat::Strict) {
                Err(FacetError::InvalidPattern { pattern, message }) => {
                    assert_eq!(pattern, nested(65));
                    assert!(message.contains("64 levels"), "{message}");
                }
                other => panic!("{version:?}: {other:?}"),
            }
        }
        assert!(facet(&"(".repeat(100_000), XsdVersion::V1_1, RegexCompat::Strict).is_err());
    }

    /// A pattern error names the pattern as written, also for an XSD 1.0
    /// pattern whose category escapes were rewritten before it was compiled.
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_pattern_errors_name_the_written_pattern() {
        for (value, version) in [
            (r"\p{Lu}(", XsdVersion::V1_0),
            (r"\d+\1", XsdVersion::V1_0),
            (r"\p{Lu}(", XsdVersion::V1_1),
        ] {
            match facet(value, version, RegexCompat::Strict) {
                Err(FacetError::InvalidPattern { pattern, message }) => {
                    assert_eq!(pattern, value);
                    assert!(
                        message.starts_with("invalid regular expression"),
                        "{message}"
                    );
                }
                other => panic!("{value}: {other:?}"),
            }
        }
    }

    /// A facet that was never compiled still matches, with the XSD 1.1
    /// defaults.
    #[cfg(feature = "xsd11")]
    #[test]
    fn test_uncompiled_facet_matches_with_xsd11_defaults() {
        let p = PatternFacet::new_unchecked(r"\p{IsNoSuchBlock}b\$?".to_string(), None);
        // `\$` is not an XSD escape: an invalid pattern matches nothing.
        assert!(!p.matches("ab"));
        let p = PatternFacet::new_unchecked(r"\p{IsNoSuchBlock}b$".to_string(), None);
        assert!(p.matches("ab$"));
        assert!(!p.matches("ab"));
    }

    // =========================================================================
    // Whitespace normalization tests
    // =========================================================================

    #[test]
    fn test_whitespace_preserve() {
        let result = normalize_whitespace("  hello\t\nworld  ", WhitespaceMode::Preserve);
        assert_eq!(&*result, "  hello\t\nworld  ");
    }

    #[test]
    fn test_whitespace_replace() {
        let result = normalize_whitespace("  hello\t\nworld  ", WhitespaceMode::Replace);
        assert_eq!(&*result, "  hello  world  ");
    }

    #[test]
    fn test_whitespace_collapse() {
        let result = normalize_whitespace("  hello\t\nworld  ", WhitespaceMode::Collapse);
        assert_eq!(&*result, "hello world");
    }

    #[test]
    fn test_whitespace_collapse_multiple_spaces() {
        let result = normalize_whitespace("a     b", WhitespaceMode::Collapse);
        assert_eq!(&*result, "a b");
    }

    #[test]
    fn test_whitespace_borrows_when_already_normalized() {
        // Preserve always borrows.
        assert!(matches!(
            normalize_whitespace("anything\t\n", WhitespaceMode::Preserve),
            Cow::Borrowed(_)
        ));
        // Replace borrows when there is no tab/CR/LF to replace.
        assert!(matches!(
            normalize_whitespace("no special ws", WhitespaceMode::Replace),
            Cow::Borrowed(_)
        ));
        // Collapse borrows when already collapsed (no edge/double spaces, no \t\r\n).
        assert!(matches!(
            normalize_whitespace("a b c", WhitespaceMode::Collapse),
            Cow::Borrowed(_)
        ));
        // Collapse also borrows the trimmed core when only leading/trailing XSD
        // whitespace differs — the core needs no internal rewriting.
        assert!(matches!(
            normalize_whitespace(" leading", WhitespaceMode::Collapse),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            normalize_whitespace("trailing ", WhitespaceMode::Collapse),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            normalize_whitespace("\n a b \t", WhitespaceMode::Collapse),
            Cow::Borrowed(_)
        ));
        // …and these still produce the correct collapsed value.
        assert_eq!(
            &*normalize_whitespace(" leading", WhitespaceMode::Collapse),
            "leading"
        );
        assert_eq!(
            &*normalize_whitespace("\n a b \t", WhitespaceMode::Collapse),
            "a b"
        );
        // Whitespace-only / empty collapse to the empty string, also borrowed.
        assert!(matches!(
            normalize_whitespace("   \t\n", WhitespaceMode::Collapse),
            Cow::Borrowed(_)
        ));
        assert_eq!(
            &*normalize_whitespace("   \t\n", WhitespaceMode::Collapse),
            ""
        );
    }

    #[test]
    fn test_whitespace_owns_when_normalization_needed() {
        // Replace owns when a tab/CR/LF must become a space.
        assert!(matches!(
            normalize_whitespace("has\ttab", WhitespaceMode::Replace),
            Cow::Owned(_)
        ));
        // Collapse owns only when the *core* needs rewriting: an internal
        // double space or an internal \t\r\n. (Leading/trailing-only whitespace
        // now borrows the trimmed core — see the borrow test above.)
        assert!(matches!(
            normalize_whitespace("a  b", WhitespaceMode::Collapse),
            Cow::Owned(_)
        ));
        assert!(matches!(
            normalize_whitespace("tab\there", WhitespaceMode::Collapse),
            Cow::Owned(_)
        ));
        // Owns when there are both edge whitespace and an internal run to squeeze.
        assert!(matches!(
            normalize_whitespace(" a  b ", WhitespaceMode::Collapse),
            Cow::Owned(_)
        ));
        assert_eq!(
            &*normalize_whitespace(" a  b ", WhitespaceMode::Collapse),
            "a b"
        );
    }

    #[test]
    fn test_whitespace_only_xsd_whitespace_not_unicode() {
        // XSD whiteSpace is exactly #x20 #x9 #xD #xA. Unicode whitespace such as
        // NBSP (U+00A0) or LINE SEPARATOR (U+2028) must be left untouched — both
        // as content and at the edges (str::trim / split_whitespace would eat it).
        let nbsp = "\u{00A0}"; // not XSD whitespace

        let collapse_in = format!(" a{nbsp}{nbsp}b ");
        let collapsed = normalize_whitespace(&collapse_in, WhitespaceMode::Collapse);
        assert_eq!(&*collapsed, format!("a{nbsp}{nbsp}b")); // edges trimmed, NBSP kept (incl. the run)
        assert!(matches!(collapsed, Cow::Borrowed(_))); // NBSP is not ws, so the core borrows

        // A leading NBSP is content, so nothing is trimmed and the whole thing borrows.
        let lead_in = format!("{nbsp}x");
        let lead = normalize_whitespace(&lead_in, WhitespaceMode::Collapse);
        assert_eq!(&*lead, format!("{nbsp}x"));
        assert!(matches!(lead, Cow::Borrowed(_)));

        // Replace only maps #x9/#xD/#xA → #x20; NBSP and multi-byte content survive.
        let repl_in = format!("café\tx{nbsp}y");
        let repl = normalize_whitespace(&repl_in, WhitespaceMode::Replace);
        assert_eq!(&*repl, format!("café x{nbsp}y"));
    }

    // =========================================================================
    // String validation tests
    // =========================================================================

    #[test]
    fn test_validate_string_length() {
        let mut facets = FacetSet::new();
        facets.set_length(5, FacetFixed::Default, None);

        assert!(facets.validate_string("hello").is_ok());
        assert!(facets.validate_string("hi").is_err());
        assert!(facets.validate_string("toolong").is_err());
    }

    #[test]
    fn test_validate_string_min_max_length() {
        let mut facets = FacetSet::new();
        facets.set_min_length(3, FacetFixed::Default, None);
        facets.set_max_length(10, FacetFixed::Default, None);

        assert!(facets.validate_string("hello").is_ok());
        assert!(facets.validate_string("hi").is_err());
        assert!(facets.validate_string("this is way too long").is_err());
    }

    #[test]
    fn test_validate_string_pattern() {
        let mut facets = FacetSet::new();
        facets
            .add_pattern(
                "[a-z]+".to_string(),
                None,
                XsdVersion::V1_1,
                RegexCompat::Strict,
            )
            .unwrap();

        assert!(facets.validate_string("hello").is_ok());
        assert!(facets.validate_string("HELLO").is_err());
    }

    #[test]
    fn test_validate_string_enumeration() {
        let mut facets = FacetSet::new();
        facets.add_enumeration("red".to_string(), None);
        facets.add_enumeration("green".to_string(), None);
        facets.add_enumeration("blue".to_string(), None);

        assert!(facets.validate_string("red").is_ok());
        assert!(facets.validate_string("yellow").is_err());
    }

    // =========================================================================
    // Decimal validation tests
    // =========================================================================

    #[test]
    fn test_validate_decimal_total_digits() {
        let mut facets = FacetSet::new();
        facets.set_total_digits(5, FacetFixed::Default, None);

        let val = Decimal::from_str("12345").unwrap();
        assert!(facets.validate_decimal(&val).is_ok());

        let val = Decimal::from_str("123456").unwrap();
        assert!(facets.validate_decimal(&val).is_err());
    }

    #[test]
    fn test_validate_decimal_fraction_digits() {
        let mut facets = FacetSet::new();
        facets.set_fraction_digits(2, FacetFixed::Default, None);

        let val = Decimal::from_str("123.45").unwrap();
        assert!(facets.validate_decimal(&val).is_ok());

        let val = Decimal::from_str("123.456").unwrap();
        assert!(facets.validate_decimal(&val).is_err());
    }

    #[test]
    fn test_validate_decimal_bounds() {
        let mut facets = FacetSet::new();
        facets.set_min_inclusive("0".to_string(), FacetFixed::Default, None);
        facets.set_max_inclusive("100".to_string(), FacetFixed::Default, None);

        let val = Decimal::from_str("50").unwrap();
        assert!(facets.validate_decimal(&val).is_ok());

        let val = Decimal::from_str("-1").unwrap();
        assert!(facets.validate_decimal(&val).is_err());

        let val = Decimal::from_str("101").unwrap();
        assert!(facets.validate_decimal(&val).is_err());
    }

    /// Datatypes 1.1 §4.3.11.3: "|i| < 10^t and 0 ≤ n ≤ t" — `0.05` is
    /// `5 / 10^2`, so it needs two total digits, not one.
    #[test]
    fn test_validate_decimal_total_digits_counts_leading_fraction_zeros() {
        let mut facets = FacetSet::new();
        facets.set_total_digits(1, FacetFixed::Default, None);
        assert!(facets
            .validate_decimal(&Decimal::from_str("0.05").unwrap())
            .is_err());
        assert!(facets
            .validate_decimal(&Decimal::from_str("0.5").unwrap())
            .is_ok());
        assert!(facets
            .validate_decimal(&Decimal::from_str("-0.500").unwrap())
            .is_ok());
        assert!(facets.validate_decimal(&Decimal::ZERO).is_ok());
        facets.set_total_digits(2, FacetFixed::Default, None);
        assert!(facets
            .validate_decimal(&Decimal::from_str("0.05").unwrap())
            .is_ok());
    }

    /// Bounds beyond what `Decimal` holds still bind (0.2.2 skipped a bound
    /// that `Decimal::from_str_exact` refused).
    #[test]
    fn test_validate_decimal_with_bounds_beyond_capacity() {
        let huge = "100000000000000000000000000000000000000";
        let mut facets = FacetSet::new();
        facets.set_min_inclusive(huge.into(), FacetFixed::Default, None);
        assert!(facets.validate_decimal(&Decimal::MAX).is_err());

        let mut facets = FacetSet::new();
        facets.set_max_inclusive(huge.into(), FacetFixed::Default, None);
        assert!(facets.validate_decimal(&Decimal::MAX).is_ok());
        facets.set_max_inclusive(format!("-{huge}"), FacetFixed::Default, None);
        assert!(facets.validate_decimal(&Decimal::MIN).is_err());

        // 1 − 10^-32: more fraction digits than `Decimal` holds.
        let below_one = "0.99999999999999999999999999999999";
        let mut facets = FacetSet::new();
        facets.set_min_exclusive(below_one.into(), FacetFixed::Default, None);
        assert!(facets.validate_decimal(&Decimal::ONE).is_ok());
        let mut facets = FacetSet::new();
        facets.set_max_exclusive(below_one.into(), FacetFixed::Default, None);
        assert!(facets.validate_decimal(&Decimal::ONE).is_err());
        // Trailing zeros past 28 places do not change the bound's value.
        let mut facets = FacetSet::new();
        facets.set_max_inclusive(
            "1.000000000000000000000000000000000".into(),
            FacetFixed::Default,
            None,
        );
        assert!(facets.validate_decimal(&Decimal::ONE).is_ok());
    }

    /// Datatypes 1.1 §4.3.7–§4.3.10: a lower bound above an upper bound is
    /// an error, "according to the datatype's order relation" — exact for
    /// `xs:decimal`, in the float or double value space for those types, in
    /// the partial orders of date/time values and durations.
    #[test]
    fn test_bound_consistency_in_the_datatype_value_space() {
        let bounds = |min: &str, max: &str| {
            let mut facets = FacetSet::new();
            facets.set_min_inclusive(min.into(), FacetFixed::Default, None);
            facets.set_max_inclusive(max.into(), FacetFixed::Default, None);
            facets
        };
        let base = FacetSet::new();
        let consistent =
            |facets: &FacetSet, datatype| facets.merge_with_base(&base, datatype).is_ok();
        // The same double, spelled two ways.
        let dbl = bounds("0.10000000000000000001", "0.1");
        assert!(consistent(&dbl, XmlTypeCode::Double));
        assert!(!consistent(&dbl, XmlTypeCode::Decimal));
        // The same float, but two doubles.
        let flt = bounds("0.1000000001", "0.1");
        assert!(consistent(&flt, XmlTypeCode::Float));
        assert!(!consistent(&flt, XmlTypeCode::Double));
        // Decimals beyond double precision.
        let dec = bounds(
            "1.00000000000000000000000000002",
            "1.00000000000000000000000000001",
        );
        assert!(!consistent(&dec, XmlTypeCode::Decimal));
        assert!(consistent(&dec, XmlTypeCode::Double));
        // Genuine conflicts in every order; special values in float/double.
        for datatype in [
            XmlTypeCode::Decimal,
            XmlTypeCode::Integer,
            XmlTypeCode::Float,
            XmlTypeCode::Double,
        ] {
            assert!(!consistent(&bounds("2", "1"), datatype), "{datatype:?}");
            assert!(consistent(&bounds("1", "2"), datatype), "{datatype:?}");
        }
        assert!(consistent(&bounds("-INF", "INF"), XmlTypeCode::Double));
        assert!(!consistent(&bounds("INF", "-INF"), XmlTypeCode::Float));
        // Without an order (no datatype) bounds are not compared.
        assert!(consistent(&bounds("2", "1"), XmlTypeCode::AnySimpleType));
        // Dates: in order, out of order, incomparable.
        assert!(consistent(
            &bounds("2000-01-01Z", "2000-01-02Z"),
            XmlTypeCode::Date
        ));
        assert!(!consistent(
            &bounds("2000-01-02Z", "2000-01-01Z"),
            XmlTypeCode::Date
        ));
        assert!(consistent(
            &bounds("2000-01-01T05:00:00", "2000-01-01T00:00:00Z"),
            XmlTypeCode::DateTime
        ));
        // Durations: P1M and P30D are incomparable, P1M and P27D are not.
        assert!(consistent(&bounds("P1M", "P30D"), XmlTypeCode::Duration));
        assert!(!consistent(&bounds("P1M", "P27D"), XmlTypeCode::Duration));
    }

    #[test]
    fn test_validate_exact_decimal() {
        use crate::types::exact_decimal::ExactDecimal;
        let x = |s: &str| ExactDecimal::parse(s).unwrap();
        let mut facets = FacetSet::new();
        facets.set_fraction_digits(28, FacetFixed::Default, None);
        facets.set_min_exclusive("1".into(), FacetFixed::Default, None);
        assert!(facets
            .validate_exact_decimal(&x("1.0000000000000000000000000001"))
            .is_ok());
        assert!(matches!(
            facets.validate_exact_decimal(&x("1.00000000000000000000000000001")),
            Err(FacetError::FractionDigitsViolation {
                actual: 29,
                max: 28
            })
        ));
        assert!(matches!(
            facets.validate_exact_decimal(&x("1.000")),
            Err(FacetError::MinExclusiveViolation { .. })
        ));
    }

    #[test]
    fn test_validate_decimal_family_enumeration_by_value() {
        use crate::types::exact_decimal::decimal_typed_value;
        let mut facets = FacetSet::new();
        facets.add_enumeration("1.0".to_string(), None);
        facets.add_enumeration("100000000000000000000000000000000000000".to_string(), None);
        let check =
            |lexical: &str| facets.validate_decimal_family(&decimal_typed_value(lexical), lexical);
        assert!(check("1.0").is_ok());
        assert!(check("1.00").is_ok());
        assert!(check("+1").is_ok());
        assert!(check("100000000000000000000000000000000000000.000").is_ok());
        assert!(check("1.1").is_err());
        assert!(check("100000000000000000000000000000000000001").is_err());
    }

    #[test]
    fn test_validate_decimal_exclusive_bounds() {
        let mut facets = FacetSet::new();
        facets.set_min_exclusive("0".to_string(), FacetFixed::Default, None);
        facets.set_max_exclusive("100".to_string(), FacetFixed::Default, None);

        let val = Decimal::from_str("0").unwrap();
        assert!(facets.validate_decimal(&val).is_err()); // 0 is not > 0

        let val = Decimal::from_str("100").unwrap();
        assert!(facets.validate_decimal(&val).is_err()); // 100 is not < 100

        let val = Decimal::from_str("50").unwrap();
        assert!(facets.validate_decimal(&val).is_ok());
    }

    // =========================================================================
    // Binary/List length validation tests
    // =========================================================================

    #[test]
    fn test_validate_binary_length() {
        let mut facets = FacetSet::new();
        facets.set_length(4, FacetFixed::Default, None);

        assert!(facets.validate_binary_length(4).is_ok());
        assert!(facets.validate_binary_length(3).is_err());
        assert!(facets.validate_binary_length(5).is_err());
    }

    #[test]
    fn test_validate_list_length() {
        let mut facets = FacetSet::new();
        facets.set_min_length(1, FacetFixed::Default, None);
        facets.set_max_length(5, FacetFixed::Default, None);

        assert!(facets.validate_list_length(3).is_ok());
        assert!(facets.validate_list_length(0).is_err());
        assert!(facets.validate_list_length(10).is_err());
    }

    // =========================================================================
    // merge_with_base tests
    // =========================================================================

    #[test]
    fn test_merge_with_base_inherits_facets() {
        let mut base = FacetSet::new();
        base.set_min_length(5, FacetFixed::Default, None);
        base.set_max_length(100, FacetFixed::Default, None);

        let derived = FacetSet::new();
        let merged = derived.merge_with_base(&base, XmlTypeCode::String).unwrap();

        assert_eq!(merged.min_length.as_ref().unwrap().value, 5);
        assert_eq!(merged.max_length.as_ref().unwrap().value, 100);
    }

    #[test]
    fn test_merge_with_base_allows_more_restrictive() {
        let mut base = FacetSet::new();
        base.set_min_length(5, FacetFixed::Default, None);
        base.set_max_length(100, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_min_length(10, FacetFixed::Default, None); // More restrictive
        derived.set_max_length(50, FacetFixed::Default, None); // More restrictive

        let merged = derived.merge_with_base(&base, XmlTypeCode::String).unwrap();
        assert_eq!(merged.min_length.as_ref().unwrap().value, 10);
        assert_eq!(merged.max_length.as_ref().unwrap().value, 50);
    }

    #[test]
    fn test_merge_with_base_rejects_less_restrictive_min_length() {
        let mut base = FacetSet::new();
        base.set_min_length(10, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_min_length(5, FacetFixed::Default, None); // Less restrictive

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_with_base_rejects_less_restrictive_max_length() {
        let mut base = FacetSet::new();
        base.set_max_length(50, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_max_length(100, FacetFixed::Default, None); // Less restrictive

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_with_base_fixed_facet_same_value_ok() {
        let mut base = FacetSet::new();
        base.set_length(10, FacetFixed::Fixed, None);

        let mut derived = FacetSet::new();
        derived.set_length(10, FacetFixed::Default, None); // Same value

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_ok());
    }

    #[test]
    fn test_merge_with_base_fixed_facet_different_value_error() {
        let mut base = FacetSet::new();
        base.set_length(10, FacetFixed::Fixed, None);

        let mut derived = FacetSet::new();
        derived.set_length(20, FacetFixed::Default, None); // Different value

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_err());
        if let Err(FacetError::FixedFacetViolation { facet_name, .. }) = result {
            assert_eq!(facet_name, "length");
        } else {
            panic!("Expected FixedFacetViolation error");
        }
    }

    #[test]
    fn test_merge_with_base_patterns_cumulative() {
        let mut base = FacetSet::new();
        base.add_pattern(
            "[a-z]+".to_string(),
            None,
            XsdVersion::V1_1,
            RegexCompat::Strict,
        )
        .unwrap();

        let mut derived = FacetSet::new();
        derived
            .add_pattern(
                "[0-9]+".to_string(),
                None,
                XsdVersion::V1_1,
                RegexCompat::Strict,
            )
            .unwrap();

        let merged = derived.merge_with_base(&base, XmlTypeCode::String).unwrap();
        // Derived step (one OR'd pattern) AND base step (one OR'd pattern)
        // = two separate AND'd steps.
        assert_eq!(merged.patterns.len(), 2);
    }

    #[test]
    fn test_merge_with_base_enumeration_subset() {
        let mut base = FacetSet::new();
        base.add_enumeration("red".to_string(), None);
        base.add_enumeration("green".to_string(), None);
        base.add_enumeration("blue".to_string(), None);

        let mut derived = FacetSet::new();
        derived.add_enumeration("red".to_string(), None);
        derived.add_enumeration("blue".to_string(), None);

        let merged = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(merged.is_ok());
    }

    #[test]
    fn test_merge_with_base_enumeration_not_subset_error() {
        let mut base = FacetSet::new();
        base.add_enumeration("red".to_string(), None);
        base.add_enumeration("green".to_string(), None);

        let mut derived = FacetSet::new();
        derived.add_enumeration("yellow".to_string(), None); // Not in base

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_with_base_whitespace_more_restrictive() {
        let mut base = FacetSet::new();
        base.set_whitespace(WhitespaceMode::Preserve, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_whitespace(WhitespaceMode::Collapse, FacetFixed::Default, None);

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_ok());
    }

    #[test]
    fn test_merge_with_base_whitespace_less_restrictive_error() {
        let mut base = FacetSet::new();
        base.set_whitespace(WhitespaceMode::Collapse, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_whitespace(WhitespaceMode::Preserve, FacetFixed::Default, None);

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_with_base_digit_facets() {
        let mut base = FacetSet::new();
        base.set_total_digits(10, FacetFixed::Default, None);
        base.set_fraction_digits(5, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_total_digits(5, FacetFixed::Default, None); // More restrictive
        derived.set_fraction_digits(2, FacetFixed::Default, None); // More restrictive

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_ok());
    }

    #[test]
    fn test_merge_with_base_digit_facets_less_restrictive_error() {
        let mut base = FacetSet::new();
        base.set_total_digits(5, FacetFixed::Default, None);

        let mut derived = FacetSet::new();
        derived.set_total_digits(10, FacetFixed::Default, None); // Less restrictive

        let result = derived.merge_with_base(&base, XmlTypeCode::String);
        assert!(result.is_err());
    }

    // =========================================================================
    // Consistency validation tests
    // =========================================================================

    #[test]
    fn test_consistency_min_greater_than_max_length() {
        let mut base = FacetSet::new();
        base.set_min_length(10, FacetFixed::Default, None);
        base.set_max_length(5, FacetFixed::Default, None);

        let result = base.merge_with_base(&FacetSet::new(), XmlTypeCode::Decimal);
        assert!(result.is_err());
    }

    #[test]
    fn test_consistency_both_inclusive_and_exclusive() {
        let mut base = FacetSet::new();
        base.set_min_inclusive("0".to_string(), FacetFixed::Default, None);
        base.set_min_exclusive("0".to_string(), FacetFixed::Default, None);

        let result = base.merge_with_base(&FacetSet::new(), XmlTypeCode::Decimal);
        assert!(result.is_err());
    }

    #[test]
    fn test_consistency_fraction_greater_than_total() {
        let mut base = FacetSet::new();
        base.set_total_digits(3, FacetFixed::Default, None);
        base.set_fraction_digits(5, FacetFixed::Default, None);

        let result = base.merge_with_base(&FacetSet::new(), XmlTypeCode::Decimal);
        assert!(result.is_err());
    }

    // =========================================================================
    // FacetKind tests
    // =========================================================================

    #[test]
    fn test_facet_kind_from_name() {
        assert_eq!(FacetKind::from_name("length"), Some(FacetKind::Length));
        assert_eq!(
            FacetKind::from_name("minLength"),
            Some(FacetKind::MinLength)
        );
        assert_eq!(FacetKind::from_name("pattern"), Some(FacetKind::Pattern));
        assert_eq!(FacetKind::from_name("unknown"), None);
    }

    #[test]
    fn test_facet_kind_name_roundtrip() {
        let kinds = [
            FacetKind::Length,
            FacetKind::MinLength,
            FacetKind::MaxLength,
            FacetKind::Pattern,
            FacetKind::Enumeration,
            FacetKind::Whitespace,
            FacetKind::MinInclusive,
            FacetKind::MaxInclusive,
            FacetKind::MinExclusive,
            FacetKind::MaxExclusive,
            FacetKind::TotalDigits,
            FacetKind::FractionDigits,
            FacetKind::ExplicitTimezone,
            FacetKind::Assertion,
        ];

        for kind in kinds {
            let name = kind.name();
            assert_eq!(FacetKind::from_name(name), Some(kind));
        }
    }

    // =========================================================================
    // XSD pattern to Rust conversion tests (XSD 1.0 path only)
    // =========================================================================

    #[cfg(not(feature = "xsd11"))]
    #[test]
    fn test_xsd_pattern_anchoring() {
        let rust = convert_xml_pattern("abc", ConvertOptions::xsd());
        assert!(rust.starts_with('^'));
        assert!(rust.ends_with('$'));
    }

    #[cfg(not(feature = "xsd11"))]
    #[test]
    fn test_xsd_pattern_initial_name_char() {
        let rust = convert_xml_pattern(r"\i", ConvertOptions::xsd());
        assert!(rust.contains("[A-Za-z_:]"));
    }

    #[cfg(not(feature = "xsd11"))]
    #[test]
    fn test_xsd_pattern_name_char() {
        let rust = convert_xml_pattern(r"\c", ConvertOptions::xsd());
        // The hyphen is escaped in the character class
        assert!(rust.contains(r"[A-Za-z0-9._:\-]"));
    }

    #[cfg(not(feature = "xsd11"))]
    #[test]
    fn test_xsd_pattern_standard_escapes() {
        let rust = convert_xml_pattern(r"\d+\s*", ConvertOptions::xsd());
        assert!(rust.contains(r"\d"));
        assert!(rust.contains(r"\s"));
    }
}
