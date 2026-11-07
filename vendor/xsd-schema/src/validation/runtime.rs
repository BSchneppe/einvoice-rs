//! `ValidationRuntime` — mutable per-run validation state.
//!
//! Created by [`SchemaValidator::start_run()`](crate::validation::validator::SchemaValidator::start_run). Holds the validation stack,
//! identity constraint tables, sink, and all other per-run mutable state.
//! Method bodies are moved verbatim from the former monolithic `SchemaValidator`.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;
use std::sync::Arc;

use ahash::AHasher;

use crate::arenas::{ComplexTypeDefData, ResolvedAttributeUse};
use crate::compiler::{compile_content_model_matcher, SubstitutionGroupMap};
use crate::ids::{
    AttributeGroupKey, AttributeKey, ComplexTypeKey, ElementKey, IdentityConstraintKey, NameId,
    NotationKey, SimpleTypeKey, TypeKey,
};
use crate::namespace::context::NamespaceContextSnapshot;
use crate::namespace::qname::{parse_qname_with_snapshot, QNameError};
use crate::namespace::table::well_known;
use crate::parser::frames::{AttributeUseKind, AttributeUseResult, IdentityKind, ProcessContents};
use crate::parser::location::{SourceLocation, SourceRef};
use crate::schema::model::DerivationSet;
use crate::schema::resolver::format_resolved_qname;
use crate::schema::SchemaSet;
use crate::types::value::XmlValue;
use crate::types::XmlTypeCode;
#[cfg(feature = "xsd11")]
use bumpalo::Bump;

use super::content::{CompiledContentModel, ContentValidatorState};
use super::context::{AttrNameSet, ElementValidationState, ValidatorState};
use super::errors::{
    self, ExpandedName, InstancePath, PathStep, ValidationError, ValidationSubject,
};
use super::identity::{CompiledIdentityConstraint, ConstraintStruct, IndexedKeyTable, KeyTable};
#[cfg(feature = "xsd11")]
use super::info::{AssertionOutcome, InheritedAttribute};
use super::info::{
    ContentProcessing, ContentType, DefaultAttribute, ExpectedAttribute, ExpectedElement,
    NoNamespaceSchemaLocationHint, SchemaInfo, SchemaLocationHint, SchemaValidity, TypeSource,
    ValidationAttempted, ValidationFlags, ValidationRoot,
};
use crate::types::complex::ProcessContents as TypesProcessContents;

#[cfg(feature = "xsd11")]
use super::assertions::{
    evaluate_complex_type_assertions, has_inherited_assertions, AssertionBufferFrame,
};
use super::expression_warnings::{self, ExpressionWarning};
#[cfg(feature = "xsd11")]
use super::validator::AssertionSource;
use super::validator::{ValidationSink, ValidationWarning};

#[cfg(feature = "xsd11")]
use crate::document::builder::BufferDocumentBuilder;
#[cfg(feature = "xsd11")]
use crate::document::BufferDocumentOptions;

// ---------------------------------------------------------------------------
// GroupAttribute — flat representation of an attribute from an attribute group
// ---------------------------------------------------------------------------

/// An attribute use collected from a resolved attribute group.
struct GroupAttribute {
    name: NameId,
    namespace: Option<NameId>,
    use_kind: AttributeUseKind,
    type_key: Option<TypeKey>,
    attr_key: Option<AttributeKey>,
    fixed_value: Option<String>,
    /// The declaration the fixed value was written on (its namespace
    /// bindings read a QName value).
    fixed_source: Option<SourceRef>,
    /// The lexical form of the use's ·effective value constraint· (`default`
    /// or `fixed`): the use's own, otherwise its declaration's.
    value: Option<String>,
    /// The element that value was written on (its namespace bindings read a
    /// QName value).
    value_source: Option<SourceRef>,
    #[cfg(feature = "xsd11")]
    inheritable: bool,
}

// ---------------------------------------------------------------------------
// AttributeUseEntry — one member of a complex type's {attribute uses}
// ---------------------------------------------------------------------------

/// One member of a complex type's {attribute uses} (Structures §3.4.2.4), as
/// [`ValidationRuntime::for_each_attribute_use`] reports it: never a
/// prohibited `<attribute>`, which "does not correspond to any component".
struct AttributeUseEntry<'s> {
    name: NameId,
    namespace: Option<NameId>,
    /// `{required}`: `Required` or `Optional`.
    use_kind: AttributeUseKind,
    /// The global declaration, when the use references one.
    attr_key: Option<AttributeKey>,
    /// The use's resolved type, otherwise its declaration's.
    type_key: Option<TypeKey>,
    /// The lexical form of the use's ·effective value constraint· — its own
    /// `default` / `fixed`, otherwise its declaration's.
    value: Option<&'s str>,
    /// The element `value` was written on: its namespace bindings read a
    /// QName value.
    value_source: Option<&'s SourceRef>,
    /// The use's `inheritable` (XSD 1.1).
    #[cfg(feature = "xsd11")]
    inheritable: bool,
}

/// A set of attribute names for one walk of the attribute uses
/// ([`ValidationRuntime::for_each_attribute_use`]): held inline while small,
/// so the walk allocates nothing for the common type, and in a hash set past
/// [`AttrNames::INLINE`] names.
struct AttrNames {
    inline: [(Option<NameId>, NameId); AttrNames::INLINE],
    len: usize,
    spilled: AttrNameSet,
}

impl AttrNames {
    const INLINE: usize = 16;

    fn new() -> Self {
        AttrNames {
            inline: [(None, NameId(0)); AttrNames::INLINE],
            len: 0,
            spilled: AttrNameSet::default(),
        }
    }

    fn contains(&self, name: &(Option<NameId>, NameId)) -> bool {
        self.inline[..self.len].contains(name) || self.spilled.contains(name)
    }

    /// Add `name`; `false` when it was already there.
    fn insert(&mut self, name: (Option<NameId>, NameId)) -> bool {
        if self.contains(&name) {
            return false;
        }
        if self.len < AttrNames::INLINE {
            self.inline[self.len] = name;
            self.len += 1;
        } else {
            self.spilled.insert(name);
        }
        true
    }

    /// Move every name of `other` into this set, leaving `other` empty.
    fn absorb(&mut self, other: &mut AttrNames) {
        for i in 0..other.len {
            self.insert(other.inline[i]);
        }
        for name in other.spilled.drain() {
            self.insert(name);
        }
        other.len = 0;
    }

    fn clear(&mut self) {
        self.len = 0;
        self.spilled.clear();
    }
}

/// A defaulted attribute value for the identity constraints: local name,
/// namespace (`NameId(0)` for none), lexical form, typed value.
type IcDefault = (NameId, NameId, String, Option<XmlValue>);

// ---------------------------------------------------------------------------
// AttributeLookup — three-state result from find_attribute_in_type
// ---------------------------------------------------------------------------

/// Result of looking up an attribute in a complex type's attribute list.
enum AttributeLookup {
    /// Found a matching attribute declaration
    /// (attr_key, type_key, fixed_value, the declaration the fixed value was
    /// written on, inheritable)
    Found(
        Option<AttributeKey>,
        Option<TypeKey>,
        Option<String>,
        Option<SourceRef>,
        bool,
    ),
    /// The attribute is explicitly prohibited
    Prohibited,
    /// No matching attribute found
    NotFound,
}

// ---------------------------------------------------------------------------
// AttributeOwnerType — the type an attribute is validated against
// ---------------------------------------------------------------------------

/// Which complex type governs an attribute, or the verdict when none can.
enum AttributeOwnerType {
    /// Validate the attribute against this complex type's attribute uses and
    /// attribute wildcard.
    Complex(ComplexTypeKey),
    /// The element admits no attribute use at all (a simple-typed element):
    /// the attribute was rejected and this is its `SchemaInfo`.
    NotAllowed(SchemaInfo),
}

// ---------------------------------------------------------------------------
// Diagnostic context — the subject and positional path of an error (B8)
// ---------------------------------------------------------------------------

/// One step of the path to the current element: its interned expanded name
/// and its 1-based position among the same-named siblings opened so far.
#[derive(Clone, Copy, Debug)]
struct RawPathStep {
    namespace: Option<NameId>,
    local_name: NameId,
    position: usize,
}

/// The item a diagnostic is about, captured for a report made later — an
/// IDREF checked by `end_validation`, a nested assertion evaluated when the
/// outermost asserted element closes, a keyref retried at an ancestor. The
/// path shares its ancestor steps with the runtime's other diagnostics.
#[derive(Clone, Debug, Default)]
pub(crate) struct ErrorContext {
    item: Option<(ValidationSubject, InstancePath)>,
}

impl ErrorContext {
    /// `err` about the recorded item, unless it names one already; with
    /// `element_path`, its local-name element path is the recorded element's.
    fn attach(&self, err: ValidationError, element_path: bool) -> ValidationError {
        let Some((subject, path)) = &self.item else {
            return err;
        };
        let mut err = if err.subject().is_none() && err.instance_path().is_none() {
            err.with_subject(subject.clone())
                .with_instance_path(path.clone())
        } else {
            err
        };
        if element_path {
            err.derive_element_path();
        }
        err
    }
}

/// Distinct names per parent that [`SiblingCounter`] finds by a linear scan
/// before it builds its direct-mapped table.
const SIBLING_LINEAR: usize = 4;

/// Slots of [`SiblingCounter`]'s direct-mapped name table (a power of two).
const SIBLING_SLOTS: usize = 64;

/// Names that lost their slot to another name, per parent, past which
/// [`SiblingCounter`] finds them by hash instead of by a scan — so a parent
/// with very many differently named children stays linear overall.
const SIBLING_COLLIDED_INDEX: usize = 32;

/// One name counted by a [`SiblingCounter`].
#[derive(Clone, Copy)]
struct SiblingEntry {
    namespace: Option<NameId>,
    local_name: NameId,
    count: usize,
    /// The table slot the name hashes to.
    slot: u8,
}

impl SiblingEntry {
    fn is(&self, namespace: Option<NameId>, local_name: NameId) -> bool {
        self.local_name == local_name && self.namespace == namespace
    }
}

/// The lookup structure of a [`SiblingCounter`] with many names.
struct SiblingTable {
    /// Entry index + 1 of the first name that hashed to each slot; 0 = empty.
    /// A name whose slot is empty has not been counted yet.
    slots: [u32; SIBLING_SLOTS],
    /// Entries whose slot was already taken by another name.
    collided: Vec<u32>,
    /// `collided` by name, kept while it holds more than
    /// [`SIBLING_COLLIDED_INDEX`] entries.
    collided_index: HashMap<(Option<NameId>, NameId), u32, BuildHasherDefault<AHasher>>,
}

/// The elements opened so far under one parent, counted per expanded name:
/// the source of the `[n]` of a path step.
///
/// Built for the common shapes at a few nanoseconds per element and a few
/// dozen bytes per depth level: the first name is held inline (a deep chain
/// of single children allocates nothing), runs of one repeated name hit
/// `last`, up to [`SIBLING_LINEAR`] names are scanned, and past that a
/// direct-mapped table (allocated once per level, kept for reuse) answers
/// "new name" in one probe. A hash map per parent, or a linear scan of every
/// name, measured 7–10 % of pure validation time on records of twenty
/// distinct children; this, under 2 %.
#[derive(Default)]
struct SiblingCounter {
    /// The first name counted (index 0).
    head: Option<SiblingEntry>,
    /// The further names, in first-seen order (index `1 + i`).
    rest: Vec<SiblingEntry>,
    /// Index of the entry counted last — siblings usually repeat.
    last: usize,
    /// The table, once this level has had more than [`SIBLING_LINEAR`] names
    /// under one parent; in use while `table_active`.
    table: Option<Box<SiblingTable>>,
    table_active: bool,
}

/// Where [`SiblingCounter::find`] found a name, or the slot of a new one.
enum SiblingLookup {
    Found(usize),
    New(u8),
}

impl SiblingCounter {
    /// Fibonacci hashing of the interned name onto [`SIBLING_SLOTS`] slots.
    fn slot_of(namespace: Option<NameId>, local_name: NameId) -> u8 {
        let key = u64::from(local_name.0) | (u64::from(namespace.map_or(u32::MAX, |n| n.0)) << 32);
        (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - SIBLING_SLOTS.trailing_zeros())) as u8
    }

    fn len(&self) -> usize {
        usize::from(self.head.is_some()) + self.rest.len()
    }

    fn entry(&self, i: usize) -> &SiblingEntry {
        if i == 0 {
            self.head.as_ref().expect("counted entry")
        } else {
            &self.rest[i - 1]
        }
    }

    fn entry_mut(&mut self, i: usize) -> &mut SiblingEntry {
        if i == 0 {
            self.head.as_mut().expect("counted entry")
        } else {
            &mut self.rest[i - 1]
        }
    }

    /// Count one more sibling named (`namespace`, `local_name`); its position.
    fn next(&mut self, namespace: Option<NameId>, local_name: NameId) -> usize {
        if self.head.is_some() {
            let last = self.entry_mut(self.last);
            if last.is(namespace, local_name) {
                last.count += 1;
                return last.count;
            }
        }
        let i = match self.find(namespace, local_name) {
            SiblingLookup::Found(i) => {
                self.entry_mut(i).count += 1;
                i
            }
            SiblingLookup::New(slot) => self.insert(namespace, local_name, slot),
        };
        self.last = i;
        self.entry(i).count
    }

    fn find(&self, namespace: Option<NameId>, local_name: NameId) -> SiblingLookup {
        if !self.table_active {
            if let Some(i) = (0..self.len()).find(|&i| self.entry(i).is(namespace, local_name)) {
                return SiblingLookup::Found(i);
            }
            return SiblingLookup::New(Self::slot_of(namespace, local_name));
        }
        let table = self.table.as_deref().expect("active table");
        let slot = Self::slot_of(namespace, local_name);
        let taken = table.slots[slot as usize];
        if taken == 0 {
            return SiblingLookup::New(slot);
        }
        let first = (taken - 1) as usize;
        if self.entry(first).is(namespace, local_name) {
            return SiblingLookup::Found(first);
        }
        let found = if table.collided.len() > SIBLING_COLLIDED_INDEX {
            table
                .collided_index
                .get(&(namespace, local_name))
                .map(|&i| i as usize)
        } else {
            table
                .collided
                .iter()
                .map(|&i| i as usize)
                .find(|&i| self.entry(i).is(namespace, local_name))
        };
        match found {
            Some(i) => SiblingLookup::Found(i),
            None => SiblingLookup::New(slot),
        }
    }

    /// Add a new name, counted once; its index.
    fn insert(&mut self, namespace: Option<NameId>, local_name: NameId, slot: u8) -> usize {
        let entry = SiblingEntry {
            namespace,
            local_name,
            count: 1,
            slot,
        };
        let i = self.len();
        if self.head.is_none() {
            self.head = Some(entry);
        } else {
            self.rest.push(entry);
        }
        if self.table_active {
            self.index_entry(i);
        } else if self.len() > SIBLING_LINEAR {
            if self.table.is_none() {
                self.table = Some(Box::new(SiblingTable {
                    slots: [0; SIBLING_SLOTS],
                    collided: Vec::new(),
                    collided_index: HashMap::default(),
                }));
            }
            self.table_active = true;
            for j in 0..self.len() {
                self.index_entry(j);
            }
        }
        i
    }

    /// File entry `i` in the active table.
    fn index_entry(&mut self, i: usize) {
        let entry = *self.entry(i);
        let table = self.table.as_deref_mut().expect("active table");
        let slot = &mut table.slots[entry.slot as usize];
        if *slot == 0 {
            *slot = i as u32 + 1;
            return;
        }
        table.collided.push(i as u32);
        if table.collided.len() > SIBLING_COLLIDED_INDEX {
            if table.collided_index.is_empty() {
                let collided = std::mem::take(&mut table.collided);
                let keys: Vec<_> = collided
                    .iter()
                    .map(|&c| {
                        let e = self.entry(c as usize);
                        ((e.namespace, e.local_name), c)
                    })
                    .collect();
                let table = self.table.as_deref_mut().expect("active table");
                table.collided_index.extend(keys);
                table.collided = collided;
            } else {
                table
                    .collided_index
                    .insert((entry.namespace, entry.local_name), i as u32);
            }
        }
    }

    /// Forget every count (the parent closed); the allocations are kept.
    fn clear(&mut self) {
        let Some(head) = self.head.take() else {
            return;
        };
        if self.table_active {
            let table = self.table.as_deref_mut().expect("active table");
            table.slots[head.slot as usize] = 0;
            for entry in &self.rest {
                table.slots[entry.slot as usize] = 0;
            }
            table.collided.clear();
            if !table.collided_index.is_empty() {
                table.collided_index.clear();
            }
            self.table_active = false;
        }
        self.rest.clear();
        self.last = 0;
    }
}

// ---------------------------------------------------------------------------
// XsiTypeOutcome — three-state result from resolve_xsi_type
// ---------------------------------------------------------------------------

/// Result of resolving an `xsi:type` attribute value.
enum XsiTypeOutcome {
    /// Successfully resolved and derivation-valid — use this type.
    Applied(TypeKey),
    /// QName invalid or type not found (cvc-elt.4.1).
    Unresolved,
    /// Type found but does not validly derive from declared type (cvc-elt.4.2).
    InvalidDerivation,
}

// ---------------------------------------------------------------------------
// ValidationRuntime
// ---------------------------------------------------------------------------

/// Mutable per-run validation state.
///
/// Created by [`super::validator::SchemaValidator::start_run()`].
/// Holds the validation stack, identity constraint tables, sink, and all
/// other per-run mutable state. The struct is explicitly `!Send + !Sync`.
pub struct ValidationRuntime<'a, S: ValidationSink> {
    /// The compiled schema set to validate against (borrowed from SchemaValidator)
    pub(crate) schema_set: &'a SchemaSet,
    /// Pre-built substitution group map (borrowed from SchemaValidator)
    pub(crate) subst_groups: &'a Option<SubstitutionGroupMap>,
    /// Validation flags controlling behaviour (copied from SchemaValidator)
    pub(crate) flags: ValidationFlags,
    /// Sink for errors and warnings
    pub sink: S,
    /// Stack of per-element validation states
    validation_stack: Vec<ElementValidationState>,
    /// Free-list of `ElementValidationState` shells recycled on element close,
    /// so a child element reuses the retained collection capacity
    /// (`text_content`, `seen_attributes`, …) of a closed sibling/ancestor
    /// instead of allocating fresh. Depth-bounded by the element nesting depth;
    /// no cap needed. See `acquire_state` / the recycle in `validate_end_element`.
    state_pool: Vec<ElementValidationState>,
    /// Current state machine state
    current_state: ValidatorState,
    /// Current source location (updated by caller)
    current_location: Option<SourceLocation>,
    /// The path to the current element: each step's expanded name and its
    /// position among its same-named siblings. The local-name path reported
    /// as [`ValidationError::element_path`] (`/root/child`) and the
    /// positional [`ValidationError::instance_path`] are both derived from it
    /// when a diagnostic is reported.
    path_steps: Vec<RawPathStep>,
    /// `sibling_counts[d]` counts the elements opened so far at path depth
    /// `d` under the current parent (cleared when that parent closes).
    sibling_counts: Vec<SiblingCounter>,
    /// The attribute being validated, for the duration of an attribute event:
    /// the subject of the diagnostics raised meanwhile.
    current_attribute: Option<(Option<NameId>, NameId)>,
    /// `path_nodes[d]` is the shared [`InstancePath`] of `path_steps[..=d]`,
    /// built on the first diagnostic that needs it (never for a valid
    /// document) and dropped when its element closes: every diagnostic under
    /// an element shares that element's path, so reporting one costs the same
    /// at any depth.
    path_nodes: Vec<InstancePath>,
    /// Names resolved for diagnostics, shared by every path and subject of
    /// the run.
    name_cache: HashMap<NameId, Arc<str>, BuildHasherDefault<AHasher>>,
    /// Pre-compiled identity constraints (lazy cache; None = compilation failed)
    compiled_constraints: HashMap<IdentityConstraintKey, Option<CompiledIdentityConstraint>>,
    /// Per-complex-type compiled content models, built once by
    /// `SchemaValidator` (see `build_content_models`) and borrowed — shared
    /// across every run. The NFA / all-group automaton is identical for every
    /// instance of a type, so each element only clones cheap `Arc`s from here
    /// and allocates fresh active states, instead of recompiling the model per
    /// element. See `PERF_ANALYZE`.
    content_models: &'a PreparedContentModels,
    /// Active constraint state instances
    active_constraints: Vec<ConstraintStruct>,
    /// Terminal operational failure (execution limit, unprepared content
    /// model). Once set, every push-API call short-circuits without
    /// assessing and [`end_validation`](Self::end_validation) returns this
    /// error, so no driver can report a successful completion. Distinct from
    /// validity: the document is neither reported valid nor invalid.
    /// See `XSD_COMPILER_REWORK.md` §8.4 / §13.3.
    operational_failure: Option<ValidationError>,
    /// Collected ID values mapped to the owner element serial.
    /// XSD 1.1 §3.17.5.2: same ID on the same owner element is allowed.
    id_values: HashMap<String, u64>,
    /// Monotonically increasing element serial counter for ID binding.
    next_element_serial: u64,
    /// Pending IDREF values: (value, location, the item that carried it)
    pending_idrefs: Vec<(String, Option<SourceLocation>, ErrorContext)>,
    /// Declared unparsed entity names from the document's DTD.
    /// When set, ENTITY/ENTITIES values are checked against this set (§3.16.4).
    unparsed_entities: Option<HashSet<String>>,
    /// Limits on the document a quick-xml driver reads in this run.
    parse_limits: crate::xml::ParseLimits,
    /// Per-element scope stack of key/unique tables: an element's slot holds
    /// the node tables of its [identity-constraint table] (Structures
    /// §3.11.5) — its own constraints' and those propagated from its children.
    ic_scope_tables: Vec<Option<HashMap<IdentityConstraintKey, IndexedKeyTable>>>,
    /// A duplicate ID (cvc-id.2) was reported since the current validation
    /// root opened; the root is then invalid (§3.3.4.3 clause 7, §3.3.4.5
    /// clause 2). Applied and cleared when the root closes.
    root_id_conflict: bool,
    /// Index into `pending_idrefs` of the first IDREF recorded since the
    /// current validation root opened: those must resolve for the root to be
    /// valid (§3.3.4.5 clause 1). Advanced when the root closes.
    root_idref_start: usize,
    /// Which assertion evaluation path is active (XSD 1.1 only)
    #[cfg(feature = "xsd11")]
    pub(crate) assertion_source: AssertionSource,
    /// Active fragment document builder (XSD 1.1).
    /// SAFETY: borrows from fragment_arena via lifetime extension. Must be
    /// dropped (.take()) before fragment_arena is reset/dropped.
    /// Declared before fragment_arena to ensure correct drop order.
    #[cfg(feature = "xsd11")]
    fragment_builder: Option<BufferDocumentBuilder<'a>>,
    /// Heap-stable bump arena for assertion fragments (XSD 1.1 only).
    #[cfg(feature = "xsd11")]
    fragment_arena: Option<Box<Bump>>,
    /// Stack of assertion buffer frames (XSD 1.1).
    #[cfg(feature = "xsd11")]
    assertion_buffer_stack: Vec<AssertionBufferFrame>,
    /// Deferred assertion frames from nested asserted elements (XSD 1.1).
    #[cfg(feature = "xsd11")]
    pending_assertion_frames: Vec<AssertionBufferFrame>,
    /// Deferred attribute PSVI results from CTA processing (XSD 1.1).
    #[cfg(feature = "xsd11")]
    deferred_attribute_results: Vec<SchemaInfo>,
    /// The defaulted attributes the last
    /// [`validate_end_of_attributes`](Self::validate_end_of_attributes) found
    /// (see [`take_default_attributes`](Self::take_default_attributes)).
    default_attributes: Vec<DefaultAttribute>,
    /// Indices into the current element's `collected_attributes` of the
    /// attributes the caller skipped (`skip_attribute`): visible to type
    /// alternatives, not validated after them (XSD 1.1).
    #[cfg(feature = "xsd11")]
    skipped_collected: Vec<usize>,
    /// The attributes of the current element that type alternatives see but
    /// that are not deferred to after the selection: the `xsi:` attributes
    /// and, under `ALLOW_XML_ATTRIBUTES`, the `xml:` ones (Structures 1.1
    /// §3.12.4 clause 1.1.2, "E's [attributes]"). Collected only for an
    /// element whose declaration has type alternatives (XSD 1.1).
    #[cfg(feature = "xsd11")]
    cta_unlisted_attributes: Vec<(Option<NameId>, NameId, String)>,
    /// Final identity constraint tables after root element close (PSVI exposure).
    final_ic_tables: Option<HashMap<IdentityConstraintKey, KeyTable>>,
    /// Accumulated `xsi:schemaLocation` hints with base URI context.
    schema_location_hints: Vec<SchemaLocationHint>,
    /// Accumulated `xsi:noNamespaceSchemaLocation` hints with base URI context.
    no_namespace_schema_location_hints: Vec<NoNamespaceSchemaLocationHint>,
    /// First-introducer serial per namespace seen during validation.
    /// Key: `Option<NameId>` (element namespace; `None` = absent namespace).
    /// Value: `element_serial` of the first element using that namespace.
    /// Used to enforce XSD 1.0 §4.3.2 Rule 4 ("late-arriving components"):
    /// an `xsi:schemaLocation` / `xsi:noNamespaceSchemaLocation` hint that
    /// announces a namespace whose first element appeared earlier than the
    /// current element is rejected (XSD 1.0 only — XSD 1.1 §G.1.15 relaxes).
    // `ahash` (ZST fixed-key build-hasher), not the default SipHash: this map is
    // `.entry()`-probed on **every element** in XSD 1.0 (see `validate_*`), and
    // the key is an interned `NameId` (not attacker-controlled hash bytes), so the
    // per-element SipHash it used to pay was pure overhead (`PERF_PROFILE_VALIDATE_ONLY`).
    first_namespace_use: HashMap<Option<NameId>, u64, BuildHasherDefault<AHasher>>,
    /// Base URI of the instance document (set by caller for relative URI resolution).
    instance_base_uri: String,
    /// How each validation root of the run is assessed (Structures §5.2);
    /// consulted only at an element start with an empty stack.
    validation_root: ValidationRoot,
    /// Cached `xs:ID` simple-type key for XSD 1.0's ct-props-correct.5
    /// "at most one ID-type attribute per element" runtime check. `None`
    /// under XSD 1.1 (the rule was removed) so the per-attribute hot path
    /// short-circuits without any built-in hash lookup or chain walk.
    cached_xsd10_id_key: Option<SimpleTypeKey>,
    /// Whether the compiled schema declares **any** inheritable attribute
    /// (XSD 1.1 §3.2.6 `{inheritable}`). Computed once from the arenas at
    /// construction. When `false`, `[inherited attributes]` is always empty,
    /// so `push_element` skips the parent→child map propagation and
    /// `validate_end_of_attributes` skips `record_inheritable_defaults`
    /// entirely — both are no-ops on such schemas (every XSD 1.0 schema, and
    /// any XSD 1.1 schema without an `inheritable="true"` attribute).
    #[cfg(feature = "xsd11")]
    has_inheritable_attributes: bool,
    /// The serial of the element whose start reported its `xsi:type` as
    /// resolving to no type (`cvc-elt.4.1`), so that its `xsi:type`
    /// attribute, invalid by XSD 1.1 §3.2.4.1 clause 5, is not reported a
    /// second time.
    xsi_type_unresolved_reported: Option<u64>,
    /// `!Send + !Sync` marker
    _not_thread_safe: PhantomData<*const ()>,
}

/// Whether the schema declares any attribute with `{inheritable}` = true
/// (XSD 1.1 §3.2.6). Scans a strict superset of every runtime read site of
/// `inheritable`: the attribute-declaration arena (wildcard-resolved
/// governing decls), every complex type's attribute uses, and every attribute
/// group's attribute uses. `false` guarantees `[inherited attributes]` stays
/// empty for every element, so the per-element propagation and default-record
/// work can be skipped without any semantic change.
#[cfg(feature = "xsd11")]
fn schema_has_inheritable_attributes(schema_set: &SchemaSet) -> bool {
    let arenas = &schema_set.arenas;
    arenas.attributes.values().any(|d| d.inheritable)
        || arenas.complex_types.values().any(|ct| {
            ct.attributes
                .iter()
                .any(|au| au.attribute.inheritable == Some(true))
        })
        || arenas.attribute_groups.values().any(|g| {
            g.attributes
                .iter()
                .any(|au| au.attribute.inheritable == Some(true))
        })
}

/// Per-complex-type compiled content models, keyed by the interned slotmap
/// `ComplexTypeKey`. Probed once per element in `init_content_model`, so it uses
/// keyed `ahash` (ZST build-hasher) rather than the default SipHash — same
/// rationale as [`first_namespace_use`]: the key is a dense interned key, not
/// attacker-controlled bytes.
///
/// [`first_namespace_use`]: ValidationRuntime::first_namespace_use
pub(crate) type ContentModelMap =
    HashMap<ComplexTypeKey, CompiledContentModel, BuildHasherDefault<AHasher>>;

/// The finished top element handed back by
/// [`ValidationRuntime::finish_top_element`]: the popped, completion-processed
/// state, plus (under XSD 1.1) the assertion outcome the value-returning
/// end-element path folds into the `SchemaInfo`.
#[cfg(feature = "xsd11")]
type FinishedElement = (ElementValidationState, Option<AssertionOutcome>);
#[cfg(not(feature = "xsd11"))]
type FinishedElement = ElementValidationState;

/// Scalar outcome of the element-start step (`start_element_by_id`), shared by
/// the value-returning and value-free entry points. `Start` carries exactly
/// the fields the start-side `SchemaInfo` can populate — every other field of
/// that struct is always empty/default at element start — so the value wrapper
/// assembles the `SchemaInfo` from it and the `_novalue` wrapper skips the
/// construction entirely. See `PERF_P1_PLAN` §Step 4.
#[derive(Clone, Copy)]
enum ElementStartOutcome {
    /// Out-of-sequence call or undeclared/unresolvable governing type under
    /// strict assessment — maps to [`SchemaInfo::invalid()`].
    Invalid,
    /// Skip- or lax-assessed element with no governing type — maps to
    /// [`SchemaInfo::empty()`].
    Empty,
    /// Assessed element start (declared, wildcard-typed, or xsi:type-governed).
    Start {
        element_decl: Option<ElementKey>,
        schema_type: Option<TypeKey>,
        validity: SchemaValidity,
        is_nil: bool,
        content_type: Option<ContentType>,
        type_source: Option<TypeSource>,
    },
}

/// What advancing the parent's content model told the element-start step about
/// a child (`ValidationRuntime::advance_parent_content_model`).
#[derive(Clone, Copy)]
enum ParentAdvanceOutcome {
    /// Nothing left to assess — the child was already pushed (skipped parent)
    /// or the run was abandoned; propagate this start outcome unchanged.
    Settled(ElementStartOutcome),
    /// Assessment continues with what the parent's content model reported.
    Advanced(ParentAdvance),
}

/// The content-model side of a child element start.
#[derive(Clone, Copy)]
struct ParentAdvance {
    /// The matching content-model term, when one matched.
    match_info: Option<super::content::ElementMatchInfo>,
    /// Whether the parent's content model accepted the child. `false` at the
    /// document root (no parent) and on a cvc-complex-type.2.4 rejection.
    content_model_accepted: bool,
}

/// The governing element declaration of a child element and how the element
/// start reached it (`ValidationRuntime::resolve_governing_declaration`).
#[derive(Clone, Copy)]
struct GoverningDeclaration {
    /// The governing declaration: the content-model match first, then the
    /// global element table — suppressed for a skip wildcard and when the
    /// content model already resolved a local element's type.
    element_key: Option<ElementKey>,
    /// The type the content model resolved for a local element, if any.
    matched_type: Option<TypeKey>,
    /// Effective `{process contents}` for this child.
    process_contents: ContentProcessing,
    /// Whether the child was matched by a content-model wildcard or open
    /// content — §3.4.6.4 dynamic EDC fires only on wildcard matches.
    #[cfg(feature = "xsd11")]
    matched_via_wildcard: bool,
}

/// The governing type of a declared element plus the element-start verdicts
/// whose diagnostics are deferred until after the child is pushed
/// (`ValidationRuntime::resolve_declared_element_type`).
#[derive(Clone, Copy)]
struct DeclaredElementBinding {
    /// Governing type after any `xsi:type` override.
    type_key: Option<TypeKey>,
    /// Where `type_key` came from.
    type_source: TypeSource,
    /// `xsi:type` was present but unresolvable or not validly derived
    /// (cvc-elt.4.1 / cvc-elt.4.2).
    xsi_type_invalid: bool,
    /// `{name}` / `{target namespace}` of the governing complex type when it is
    /// abstract, for the cvc-type.2 diagnostic.
    abstract_ct_info: Option<(Option<NameId>, Option<NameId>)>,
    /// §3.3.4.4 cvc-type clause 2 verdict (deferred to CTA under XSD 1.1).
    abstract_type_invalid: bool,
    /// The element declaration is abstract (cvc-elt.2).
    is_abstract: bool,
    /// The declaration carries an unresolved compile-time `type` (§src-resolve).
    has_deferred_type_error: bool,
    /// `xsi:nil` selects the nilled interpretation.
    is_nil: bool,
    /// `xsi:nil` present on a non-nillable declaration (cvc-elt.3.1).
    nillable_violation: bool,
    /// The element is ·nilled· but its declaration has a fixed value
    /// constraint (cvc-elt.3.2.2).
    nil_fixed_violation: bool,
    /// The declaration carries type alternatives, so the governing type is only
    /// final after conditional type assignment (§3.12.4).
    #[cfg(feature = "xsd11")]
    has_type_alternatives: bool,
}

/// Record that an element's `[validity]` is invalid because of something other
/// than its own local checks — an invalid child (Structures §3.3.5.1
/// `[validity]` clause 1.1.2), a violated identity constraint it carries
/// (§3.3.4.3 clause 6), or, for the validation root, an ID/IDREF violation
/// (§3.3.4.3 clause 7, §3.3.4.5). Only a `valid` verdict turns `invalid`: an
/// element that was not strictly assessed is `notKnown` whatever its children
/// (§3.3.5.1 clause 2), and an `invalid` one stays so.
fn invalidate(validity: &mut SchemaValidity) {
    if *validity == SchemaValidity::Valid {
        *validity = SchemaValidity::Invalid;
    }
}

/// The `[validity]` of a closing element (Structures §3.3.5.1): "1 If it was
/// ·strictly assessed·, then … valid … otherwise invalid. 2 otherwise
/// notKnown." An element that is laxly assessed — no governing element
/// declaration nor type, in a lax context (§3.3.4.6 clause 3) — is notKnown
/// whatever was found among its attributes and children (an invalid attribute
/// or descendant is reported and is invalid itself); XSD 1.0 §3.3.5 alike.
/// Under a strict wildcard (or the default strict root) such an element is an
/// error, `cvc-elt.1`, and keeps the runtime's `Invalid`.
fn assessed_validity(ev_state: &ElementValidationState) -> SchemaValidity {
    if !ev_state.strictly_assessed && ev_state.process_contents == ContentProcessing::Lax {
        SchemaValidity::NotKnown
    } else {
        ev_state.validity
    }
}

/// Element-start step 9: initialize the freshly acquired child state for a
/// declared element. The caller has already seeded the state's namespace
/// context and content model; this sets the governing binding, the nil flag,
/// the validity verdict and the assessment mode.
fn init_declared_element_state(
    ev_state: &mut ElementValidationState,
    elem_key: ElementKey,
    binding: DeclaredElementBinding,
    process_contents: ContentProcessing,
    content_type: ContentType,
    start_invalid: bool,
) {
    let DeclaredElementBinding {
        type_key,
        type_source,
        is_nil,
        ..
    } = binding;
    ev_state.element_decl = Some(elem_key);
    ev_state.schema_type = type_key;
    ev_state.type_source = Some(type_source);
    ev_state.content_type = Some(content_type);
    ev_state.is_nil = is_nil;
    ev_state.validity = binding.state_validity(start_invalid);
    ev_state.process_contents = process_contents;
    // Strictly assessed: has governing declaration or type, and not skipped
    ev_state.strictly_assessed = (ev_state.element_decl.is_some()
        || ev_state.schema_type.is_some())
        && process_contents != ContentProcessing::Skip;
    #[cfg(feature = "xsd11")]
    {
        ev_state.has_type_alternatives = binding.has_type_alternatives;
    }
}

impl DeclaredElementBinding {
    /// The start-side verdict, recorded both on the pushed
    /// [`ElementValidationState`] and in the `SchemaInfo` the start event
    /// returns — the two must agree.
    ///
    /// `nillable_violation` is one of them. Element Locally Valid (Element)
    /// (§3.3.4.2) makes it a verdict on the element itself: "For an element
    /// information item E to be locally ·valid· with respect to an element
    /// declaration D all of the following must be true: … 3 One of the
    /// following is true: 3.1 D . {nillable} = false, and E has no xsi:nil
    /// attribute." An `xsi:nil` on a non-nillable declaration satisfies
    /// neither clause 3.1 nor clause 3.2, so E is not locally valid and its
    /// `[validity]` is `invalid` from the start event onward.
    ///
    /// So is `nil_fixed_violation` (cvc-elt.3.2.2): "3.2 D.{nillable} = true
    /// and one of the following is true … 3.2.3 E has xsi:nil = true (that
    /// is, E is ·nilled·), and all of the following are true: … 3.2.3.2 D has
    /// no {value constraint} with {variety} = fixed." A ·nilled· element of a
    /// declaration with a fixed value satisfies none of 3.2.1 – 3.2.3.
    ///
    /// So is `is_abstract` (cvc-elt.2): the same rule's "2 D.{abstract} =
    /// false." `start_invalid` carries the verdicts found beside the binding:
    /// a dynamic EDC violation (XSD 1.1), and a declaration stipulated for the
    /// validation root whose name is not the element's (clause "1 D is not
    /// ·absent· and E and D have the same expanded name").
    fn state_validity(&self, start_invalid: bool) -> SchemaValidity {
        if self.xsi_type_invalid
            || self.is_abstract
            || self.abstract_type_invalid
            || self.nillable_violation
            || self.nil_fixed_violation
            || self.has_deferred_type_error
            || start_invalid
        {
            SchemaValidity::Invalid
        } else {
            SchemaValidity::Valid
        }
    }
}

/// Scalar outcome of the end-of-attributes step (`end_of_attributes_inner`),
/// shared by the value-returning and value-free entry points; same pattern as
/// [`ElementStartOutcome`].
enum EndOfAttributesOutcome {
    /// Out-of-sequence call — maps to [`SchemaInfo::invalid()`].
    Invalid,
    /// Normal completion — maps to [`SchemaInfo::empty()`].
    Done,
    /// CTA evaluated and selected (and possibly switched) the governing type
    /// (XSD 1.1): the value wrapper rebuilds the type/content/validity info
    /// from the stack top so callers (e.g. the typed builder) can update
    /// element bindings.
    #[cfg(feature = "xsd11")]
    CtaUpdated,
}

/// Build the per-complex-type compiled content models once, at
/// `SchemaValidator` construction. Mirrors how substitution groups are built
/// once and reused across runs (`build_substitution_group_map`).
///
/// Only element-only / mixed types get a content model — exactly the set the
/// per-element path consults — so no work is wasted on empty / simple-content
/// types. The heavy automaton lives behind `Arc`s in [`CompiledContentModel`],
/// so each element instance clones cheap handles instead of recompiling the NFA.
///
/// Compilation failures are simply omitted from the map; the runtime then falls
/// back to empty content (matching the previous per-element error path) and
/// never retries the failed compile.
/// Load-time content-model preparation: the compiled models plus every
/// complex type whose model could **not** be prepared, with the reason.
///
/// A failed type is deliberately not treated as empty content. The first
/// element it governs raises an operational failure
/// (`validation-preparation-failed`) that aborts the run — see
/// [`ValidationRuntime::operational_failure`] — and
/// `SchemaValidator::content_model_failures` exposes the list up front.
pub(crate) struct PreparedContentModels {
    pub(crate) models: ContentModelMap,
    pub(crate) failures: HashMap<ComplexTypeKey, String, BuildHasherDefault<AHasher>>,
}

pub(crate) fn build_content_models(schema_set: &SchemaSet) -> PreparedContentModels {
    let mut models = ContentModelMap::default();
    let mut failures: HashMap<ComplexTypeKey, String, BuildHasherDefault<AHasher>> =
        HashMap::default();
    for (ct_key, ct_data) in schema_set.arenas.complex_types.iter() {
        if matches!(
            determine_content_type(schema_set, ct_data),
            ContentType::ElementOnly | ContentType::Mixed
        ) {
            match compile_content_model_matcher(schema_set, ct_data) {
                Ok(matcher) => match CompiledContentModel::try_from_matcher(matcher) {
                    Ok(model) => {
                        models.insert(ct_key, model);
                    }
                    Err(limit) => {
                        failures.insert(ct_key, limit.to_string());
                    }
                },
                Err(err) => {
                    failures.insert(ct_key, format!("{err}"));
                }
            }
        }
    }
    PreparedContentModels { models, failures }
}

/// Determine the [`ContentType`] of a complex type from its `ComplexTypeDefData`.
///
/// Cheap and allocation-free; shared by the per-run content-model lookup in
/// [`ValidationRuntime::init_content_model`] and the schema-load-time content
/// model build in [`build_content_models`].
pub(crate) fn determine_content_type(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
) -> ContentType {
    use crate::parser::frames::ComplexContentResult;
    use crate::parser::frames::DerivationMethod;
    match &ct_data.content {
        ComplexContentResult::Empty => {
            // §3.4.2.3 step 5.2: a `<defaultOpenContent>` with
            // `appliesToEmpty="true"` widens an otherwise-empty
            // content type to element-only/mixed (XSD 1.1).
            #[cfg(feature = "xsd11")]
            if schema_set.is_xsd11()
                && empty_ct_has_applicable_default_open_content(schema_set, ct_data)
            {
                return if ct_data.mixed {
                    ContentType::Mixed
                } else {
                    ContentType::ElementOnly
                };
            }
            if ct_data.mixed {
                ContentType::Mixed
            } else {
                ContentType::Empty
            }
        }
        ComplexContentResult::Simple(_) => ContentType::TextOnly,
        ComplexContentResult::Complex(def) => {
            // §3.4.2.3 step 4.1.1 / 4.2.1: explicit content type variety
            // = empty when the effective content is empty (the explicit
            // content is empty *and* effective mixed is false). Step 6
            // then promotes variety to element-only/mixed if a wildcard
            // element (open content) is present — without one, the final
            // {content type} stays empty (no character or element items
            // are allowed). Only apply the relaxed §3.4.2.3 step 2.1.x
            // particle-shape recognition under XSD 1.1; XSD 1.0 tests
            // historically rely on `def.particle.is_none()` strictness.
            // XSD 1.0 has no clause 2.1.4, but a particle with minOccurs =
            // maxOccurs = 0 "corresponds to no component at all" there too
            // (§3.7.2, §3.8.2): no particle, empty content (white space is
            // refused, as the content model matcher has nothing to match).
            let particle_empty = match &def.particle {
                None => true,
                Some(p) if schema_set.is_xsd11() => {
                    crate::parser::frames::particle_is_explicit_empty(p)
                }
                Some(p) => p.min_occurs == 0 && p.max_occurs == Some(0),
            };
            let has_open_content = type_has_effective_open_content(schema_set, ct_data, def);
            if particle_empty && !ct_data.mixed && !def.mixed && !has_open_content {
                // For extensions with no own particle, inherit base type's content type
                if matches!(ct_data.derivation_method, Some(DerivationMethod::Extension)) {
                    if let Some(TypeKey::Complex(base_ct_key)) = ct_data.resolved_base_type {
                        let base_data = &schema_set.arenas.complex_types[base_ct_key];
                        return determine_content_type(schema_set, base_data);
                    }
                }
                ContentType::Empty
            } else if ct_data.mixed || def.mixed {
                ContentType::Mixed
            } else {
                ContentType::ElementOnly
            }
        }
    }
}

/// Whether an otherwise-empty (`ComplexContentResult::Empty`) complex type
/// effectively gains a wildcard via the schema document's
/// `<defaultOpenContent>` with `appliesToEmpty="true"`. Mirrors the
/// `appliesToEmpty` half of [`type_has_effective_open_content`] for the
/// shorter-syntax CTs that don't carry a `ComplexContentDefResult`.
#[cfg(feature = "xsd11")]
fn empty_ct_has_applicable_default_open_content(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
) -> bool {
    let doc = ct_data
        .source
        .as_ref()
        .and_then(|s| schema_set.documents.get(s.defaults_doc() as usize));
    if let Some(default) = doc.and_then(|d| d.default_open_content.as_ref()) {
        if default.mode == crate::schema::model::OpenContentMode::None {
            return false;
        }
        return default.applies_to_empty;
    }
    false
}

/// Whether a complex type effectively has open content per §3.4.2.3 step 5
/// (the "wildcard element"). Used by [`determine_content_type`] to distinguish
/// a truly empty content type from one whose explicit content type variety =
/// empty but is widened to element-only by an open content wildcard (own
/// `<openContent>` or applicable `<defaultOpenContent>`).
#[allow(unused_variables)]
fn type_has_effective_open_content(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
    def: &crate::parser::frames::ComplexContentDefResult,
) -> bool {
    #[cfg(feature = "xsd11")]
    {
        use crate::parser::frames::OpenContentMode;
        // Step 5.1: own <openContent> child with mode != none.
        if let Some(oc) = def.open_content.as_ref() {
            return oc.mode != OpenContentMode::None;
        }
        // Step 5.2: schema's <defaultOpenContent>. Applies when
        // appliesToEmpty=true OR explicit content type variety ≠ empty.
        // Use the same gate as compile.rs `resolve_open_content`.
        let doc = ct_data
            .source
            .as_ref()
            .and_then(|s| schema_set.documents.get(s.defaults_doc() as usize));
        if let Some(default) = doc.and_then(|d| d.default_open_content.as_ref()) {
            if default.mode == crate::schema::model::OpenContentMode::None {
                return false;
            }
            if default.applies_to_empty {
                return true;
            }
            return !ct_data.content.explicit_content_type_is_empty();
        }
    }
    let _ = (schema_set, ct_data, def);
    false
}

impl<'a, S: ValidationSink> ValidationRuntime<'a, S> {
    /// Create a new `ValidationRuntime` (called by `SchemaValidator::start_run()`).
    pub(crate) fn new(
        schema_set: &'a SchemaSet,
        subst_groups: &'a Option<SubstitutionGroupMap>,
        content_models: &'a PreparedContentModels,
        flags: ValidationFlags,
        sink: S,
        #[cfg(feature = "xsd11")] assertion_source: AssertionSource,
    ) -> Self {
        let cached_xsd10_id_key = if schema_set.is_xsd10() {
            schema_set.builtin_types().get_by_type_code(XmlTypeCode::Id)
        } else {
            None
        };
        #[cfg(feature = "xsd11")]
        let has_inheritable_attributes = schema_has_inheritable_attributes(schema_set);
        ValidationRuntime {
            schema_set,
            subst_groups,
            content_models,
            flags,
            sink,
            validation_stack: Vec::new(),
            state_pool: Vec::new(),
            current_state: ValidatorState::None,
            current_location: None,
            path_steps: Vec::new(),
            sibling_counts: Vec::new(),
            current_attribute: None,
            path_nodes: Vec::new(),
            name_cache: HashMap::default(),
            compiled_constraints: HashMap::new(),
            active_constraints: Vec::new(),
            operational_failure: None,
            id_values: HashMap::new(),
            next_element_serial: 0,
            pending_idrefs: Vec::new(),
            unparsed_entities: None,
            parse_limits: crate::xml::ParseLimits::none(),
            ic_scope_tables: Vec::new(),
            root_id_conflict: false,
            root_idref_start: 0,
            final_ic_tables: None,
            schema_location_hints: Vec::new(),
            no_namespace_schema_location_hints: Vec::new(),
            first_namespace_use: HashMap::default(),
            instance_base_uri: String::new(),
            validation_root: ValidationRoot::Declaration,
            #[cfg(feature = "xsd11")]
            assertion_source,
            #[cfg(feature = "xsd11")]
            fragment_builder: None,
            #[cfg(feature = "xsd11")]
            fragment_arena: None,
            #[cfg(feature = "xsd11")]
            assertion_buffer_stack: Vec::new(),
            #[cfg(feature = "xsd11")]
            pending_assertion_frames: Vec::new(),
            #[cfg(feature = "xsd11")]
            deferred_attribute_results: Vec::new(),
            default_attributes: Vec::new(),
            #[cfg(feature = "xsd11")]
            skipped_collected: Vec::new(),
            #[cfg(feature = "xsd11")]
            cta_unlisted_attributes: Vec::new(),
            cached_xsd10_id_key,
            #[cfg(feature = "xsd11")]
            has_inheritable_attributes,
            xsi_type_unresolved_reported: None,
            _not_thread_safe: PhantomData,
        }
    }

    /// Set the current source location for error reporting
    pub fn set_location(&mut self, location: SourceLocation) {
        self.current_location = Some(location);
    }

    /// Clear the current source location
    pub fn clear_location(&mut self) {
        self.current_location = None;
    }

    /// Returns the final identity constraint tables after validation completes.
    ///
    /// Only available after `end_validation()` succeeds. Contains key/unique/keyref
    /// tables accumulated during the root element's validation scope.
    pub fn identity_constraint_tables(&self) -> Option<&HashMap<IdentityConstraintKey, KeyTable>> {
        self.final_ic_tables.as_ref()
    }

    /// Set the declared unparsed entity names from the document's DTD.
    ///
    /// When set, ENTITY/ENTITIES values are validated against this set per
    /// §3.16.4 String Valid clause 3: "Every ENTITY value in V is a declared
    /// entity name."
    pub fn set_unparsed_entities(&mut self, entities: HashSet<String>) {
        self.unparsed_entities = Some(entities);
    }

    /// Set the limits on the document read in this run.
    ///
    /// Honoured by the quick-xml drivers ([`drive_quick_xml`] and its
    /// variants) and by the typed build over them
    /// (`document::build_typed_document_with`): each limit is checked on the
    /// raw markup before the driver interns or validates anything of it, and
    /// a document that breaks one is refused with
    /// [`DriveWithError::LimitExceeded`] / [`DriveError::LimitExceeded`]
    /// (see [`ParseLimits`](crate::xml::ParseLimits)). The tree-walking
    /// drivers read no text and ignore them. The default is no limits.
    ///
    /// [`drive_quick_xml`]: crate::validation::drive_quick_xml
    /// [`DriveWithError::LimitExceeded`]: crate::validation::DriveWithError::LimitExceeded
    /// [`DriveError::LimitExceeded`]: crate::validation::DriveError::LimitExceeded
    pub fn set_parse_limits(&mut self, limits: crate::xml::ParseLimits) {
        self.parse_limits = limits;
    }

    /// The limits on the document read in this run (see
    /// [`set_parse_limits`](Self::set_parse_limits)).
    pub fn parse_limits(&self) -> &crate::xml::ParseLimits {
        &self.parse_limits
    }

    /// Set the base URI of the instance document being validated.
    ///
    /// This base URI is attached to every schema-location hint collected
    /// during validation so that relative URIs can be resolved correctly
    /// when schemas are loaded later.
    pub fn set_instance_base_uri(&mut self, base_uri: impl Into<String>) {
        self.instance_base_uri = base_uri.into();
    }

    /// Choose how the run assesses its validation roots (Structures §5.2):
    /// by the top-level declaration of the root's name (the default,
    /// [`ValidationRoot::Declaration`]), against a stipulated element
    /// declaration or type definition, or laxly when the root is undeclared —
    /// see [`ValidationRoot`] for what each mode does to the root's
    /// [`SchemaInfo`].
    ///
    /// Call it before the root's element event. It takes effect at the next
    /// element started while no element is open, and holds for every such
    /// root until it is set again. A stipulated declaration or type must
    /// belong to this run's schema set: one the set does not hold makes the
    /// root's element event an operational failure
    /// (`validation-root-unknown`, see
    /// [`operational_failure`](Self::operational_failure)), returned by
    /// [`end_validation`](Self::end_validation).
    ///
    /// # Example
    ///
    /// Validate an element against a named type, whatever its name:
    ///
    /// ```
    /// use xsd_schema::validation::{
    ///     drive_quick_xml, CollectingValidationSink, SchemaValidator, SchemaValidity,
    ///     ValidationFlags, ValidationRoot,
    /// };
    /// use xsd_schema::SchemaSetBuilder;
    ///
    /// let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    ///   <xs:complexType name="point">
    ///     <xs:attribute name="x" type="xs:int" use="required"/>
    ///   </xs:complexType>
    /// </xs:schema>"#;
    /// let schema_set = SchemaSetBuilder::new()
    ///     .add_source(xsd, "file:///point.xsd")?
    ///     .compile()?
    ///     .into_schema_set();
    /// let point = schema_set
    ///     .lookup_type(None, schema_set.name_table.add("point"))
    ///     .expect("a named type");
    /// let validator = SchemaValidator::new(&schema_set, ValidationFlags::default());
    ///
    /// let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    /// let mut runtime = validator.start_run(CollectingValidationSink {
    ///     errors: &mut errors,
    ///     warnings: &mut warnings,
    /// });
    /// runtime.set_validation_root(ValidationRoot::Type(point));
    /// let outcome = drive_quick_xml(r#"<here x="3"/>"#.as_bytes(), &mut runtime, &schema_set)?;
    /// assert_eq!(outcome.root_validity, Some(SchemaValidity::Valid));
    ///
    /// // Lax: an undeclared root is not an error; its [validity] is notKnown.
    /// let mut runtime = validator.start_run(CollectingValidationSink {
    ///     errors: &mut errors,
    ///     warnings: &mut warnings,
    /// });
    /// runtime.set_validation_root(ValidationRoot::Lax);
    /// let outcome = drive_quick_xml("<anything/>".as_bytes(), &mut runtime, &schema_set)?;
    /// assert_eq!(outcome.root_validity, Some(SchemaValidity::NotKnown));
    /// drop(runtime);
    /// assert!(errors.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set_validation_root(&mut self, root: ValidationRoot) {
        self.validation_root = root;
    }

    /// How the run assesses its validation roots (see
    /// [`set_validation_root`](Self::set_validation_root)).
    pub fn validation_root(&self) -> ValidationRoot {
        self.validation_root
    }

    /// Returns accumulated `xsi:schemaLocation` hints.
    ///
    /// Each hint contains a namespace/location pair plus the instance base
    /// URI for resolving relative locations. Complete pairs from every
    /// `xsi:schemaLocation` attribute are included, even from attributes
    /// that failed even-token-count enforcement (the complete pairs are
    /// still valid hints). Any trailing unpaired token is ignored.
    pub fn schema_location_hints(&self) -> &[SchemaLocationHint] {
        &self.schema_location_hints
    }

    /// Returns accumulated `xsi:noNamespaceSchemaLocation` hints.
    pub fn no_namespace_schema_location_hints(&self) -> &[NoNamespaceSchemaLocationHint] {
        &self.no_namespace_schema_location_hints
    }

    /// Returns a reference to the fragment arena, if it has been allocated.
    #[cfg(feature = "xsd11")]
    #[cfg(test)]
    fn fragment_arena(&self) -> Option<&Bump> {
        self.fragment_arena.as_deref()
    }

    /// Returns a mutable reference to the fragment arena, allocating it on
    /// first use.
    ///
    /// # Panics (debug)
    /// Panics if `fragment_builder` is active, which would mean the arena
    /// is borrowed and mutation would invalidate the builder's reference.
    #[cfg(feature = "xsd11")]
    #[cfg(test)]
    fn fragment_arena_mut(&mut self) -> &mut Bump {
        debug_assert!(
            self.fragment_builder.is_none(),
            "fragment_arena_mut() called while fragment_builder is active — \
             would invalidate the builder's borrow"
        );
        self.fragment_arena
            .get_or_insert_with(|| Box::new(Bump::new()))
    }

    // -----------------------------------------------------------------------
    // Assertion buffering helpers (XSD 1.1)
    // -----------------------------------------------------------------------

    /// Returns `true` if assertion buffering is currently active.
    #[cfg(feature = "xsd11")]
    #[inline]
    fn is_buffering_assertions(&self) -> bool {
        !self.assertion_buffer_stack.is_empty()
    }

    /// Creates the arena + builder when the outermost asserted element is
    /// encountered. Returns `false` if builder creation fails.
    #[cfg(feature = "xsd11")]
    fn begin_assertion_buffering(&mut self) -> bool {
        let arena_box = self
            .fragment_arena
            .get_or_insert_with(|| Box::new(Bump::new()));
        // SAFETY: Box<Bump> is heap-allocated — stable address across struct moves.
        // The builder will be dropped (via .take()) before the arena is reset or dropped.
        // Field declaration order guarantees fragment_builder drops before fragment_arena.
        let arena_ref: &'a Bump = unsafe { &*(&**arena_box as *const Bump) };
        let names_ref: &'a crate::namespace::table::NameTable = &self.schema_set.name_table;
        // Pass the SchemaSet through so navigator.typed_value() can resolve
        // bindings against the schema arenas. Without it the navigator
        // short-circuits to TypedValue::Untyped (navigator.rs:683-686),
        // and assertion XPath sees every typed attribute/element as
        // xs:untypedAtomic — value-comparison operators then fail with
        // "op:eq is not defined for xs:untypedAtomic and ...".
        match BufferDocumentBuilder::new(
            arena_ref,
            names_ref,
            Some(self.schema_set),
            BufferDocumentOptions::fragment(),
        ) {
            Ok(builder) => {
                self.fragment_builder = Some(builder);
                true
            }
            Err(e) => {
                self.report_error(
                    "cvc-assertion",
                    format!("Failed to create assertion fragment builder: {}", e),
                );
                false
            }
        }
    }

    /// Abort assertion buffering due to a builder error. Drops the builder,
    /// clears all assertion state, and resets the arena. Called when a
    /// forwarding operation (attribute/end_element) fails, to prevent a
    /// desynchronized fragment from producing wrong assertion results.
    #[cfg(feature = "xsd11")]
    fn abort_assertion_buffering(&mut self, error_msg: String) {
        self.report_error("cvc-assertion", error_msg);
        // Drop builder before resetting arena (maintains safety invariant)
        self.fragment_builder.take();
        self.assertion_buffer_stack.clear();
        self.pending_assertion_frames.clear();
        if let Some(arena) = self.fragment_arena.as_mut() {
            arena.reset();
        }
    }

    /// Unified message format for assertion-fragment-buffer aborts.
    #[cfg(feature = "xsd11")]
    fn abort_assertion_buffer_op(&mut self, context: &str, e: impl std::fmt::Display) {
        self.abort_assertion_buffering(format!(
            "Assertion fragment buffer error ({}): {}",
            context, e
        ));
    }

    /// Install a schema binding on a buffered fragment node, aborting
    /// the buffer with a labelled error on failure. Returns `false` if
    /// the buffer was aborted, so callers can short-circuit.
    #[cfg(feature = "xsd11")]
    fn install_fragment_binding(
        &mut self,
        node_ref: u32,
        binding: crate::document::type_remap::NodeSchemaBinding,
        context: &str,
    ) -> bool {
        let Some(builder) = self.fragment_builder.as_mut() else {
            return true;
        };
        if let Err(e) = builder.set_node_binding(node_ref, binding) {
            self.abort_assertion_buffer_op(context, e);
            return false;
        }
        true
    }

    /// Called after every `push_element()`. Detects whether the element's
    /// complex type has assertions. If so, starts or extends assertion
    /// buffering. Also forwards `start_element` to the builder for all
    /// children within an active buffered scope.
    #[cfg(feature = "xsd11")]
    fn detect_assertions_on_element(
        &mut self,
        type_key: Option<TypeKey>,
        local_name: NameId,
        namespace: Option<NameId>,
    ) {
        if self.assertion_source != AssertionSource::FragmentBuffer {
            return;
        }
        let has_assertions = match type_key {
            Some(TypeKey::Complex(ct_key))
                if has_inherited_assertions(ct_key, &self.schema_set.arenas) =>
            {
                Some(ct_key)
            }
            _ => None,
        };

        let force_start = self
            .validation_stack
            .last()
            .is_some_and(|ev| ev.has_type_alternatives);

        if !self.is_buffering_assertions() && has_assertions.is_none() && !force_start {
            return; // nothing to do
        }

        // Start buffering if this is the outermost asserted element
        if !self.is_buffering_assertions() && !self.begin_assertion_buffering() {
            return; // builder creation failed — error already reported
        }

        // Forward start_element to builder (all children in scope).
        // Materialize ns_declarations from the live in-scope bindings so the
        // fragment element exposes the same namespace nodes, enabling
        // `fn:in-scope-prefixes()` and prefixed-QName assertions to see
        // every prefix declared by the element or its ancestors.
        let local = self.schema_set.name_table.resolve(local_name);
        let ns = namespace
            .map(|id| self.schema_set.name_table.resolve(id).to_string())
            .unwrap_or_default();
        let mut ns_strings: Vec<(String, String)> = Vec::new();
        if let Some(snapshot) = self
            .validation_stack
            .last()
            .and_then(|ev| ev.ns_context.as_ref())
        {
            for &(prefix_id, uri_id) in &snapshot.bindings {
                let prefix = self.schema_set.name_table.resolve(prefix_id).to_string();
                let uri = self.schema_set.name_table.resolve(uri_id).to_string();
                ns_strings.push((prefix, uri));
            }
            if let Some(default_ns) = snapshot.default_ns {
                let uri = self.schema_set.name_table.resolve(default_ns).to_string();
                ns_strings.push((String::new(), uri));
            }
        }
        let ns_decls: Vec<(&str, &str)> = ns_strings
            .iter()
            .map(|(p, u)| (p.as_str(), u.as_str()))
            .collect();
        let element_ref = match self.fragment_builder.as_mut() {
            Some(builder) => match builder.start_element(&local, &ns, "", &ns_decls) {
                Ok(r) => r,
                Err(e) => {
                    self.report_error(
                        "cvc-assertion",
                        format!("Assertion fragment buffer error (start_element): {}", e),
                    );
                    return;
                }
            },
            None => return, // builder was not created (error already reported)
        };

        // Install schema binding for descendant elements so assertion
        // XPath sees their declared type. The asserter element itself
        // is left unbound — per §3.13.4.1 (note around clause 2.3.1.3),
        // it has annotation `anyType`, so `data(.)` and `string(.)`
        // yield xs:untypedAtomic; the typed value is exposed via
        // `$value`. Observable in saxonData/Assert/assert014–017.
        if has_assertions.is_none() {
            if let Some(tk) = type_key {
                let (element_decl, content_type) = self
                    .validation_stack
                    .last()
                    .map(|ev| (ev.element_decl, ev.content_type))
                    .unwrap_or((None, None));
                let binding = crate::document::type_remap::NodeSchemaBinding {
                    type_key: tk,
                    element_decl,
                    attribute_decl: None,
                    content_type,
                };
                if !self.install_fragment_binding(element_ref, binding, "element binding") {
                    return;
                }
            }
        }

        // Save element_ref for potential CTA re-detection
        if let Some(ev) = self.validation_stack.last_mut() {
            ev.assertion_element_ref = Some(element_ref);
        }

        // Push assertion frame if this element has assertions
        if let Some(ct_key) = has_assertions {
            self.assertion_buffer_stack.push(AssertionBufferFrame {
                element_ref,
                complex_type_key: ct_key,
                context: ErrorContext::default(), // populated at end-element
                location: None,                   // populated at end-element
                propagates_to_outer: false,       // populated at deferral
            });
            if let Some(ev) = self.validation_stack.last_mut() {
                ev.owns_assertion_buffer = true;
            }
        }
    }

    /// Re-detect assertions after CTA switched the type, without
    /// re-emitting `start_element` on the fragment builder. Pops any
    /// stale assertion frame for the old type and pushes a new one
    /// for the new type if it carries inherited assertions.
    #[cfg(feature = "xsd11")]
    fn redetect_assertions_after_cta(&mut self, new_type: Option<TypeKey>) {
        if self.assertion_source != AssertionSource::FragmentBuffer {
            return;
        }

        // Pop old assertion frame, saving its element_ref
        let old_element_ref = if let Some(ev) = self.validation_stack.last_mut() {
            if ev.owns_assertion_buffer {
                let frame = self.assertion_buffer_stack.pop();
                ev.owns_assertion_buffer = false;
                frame.map(|f| f.element_ref)
            } else {
                None
            }
        } else {
            return;
        };

        // Check if new type has assertions
        let new_ct_key = match new_type {
            Some(TypeKey::Complex(ct_key))
                if has_inherited_assertions(ct_key, &self.schema_set.arenas) =>
            {
                ct_key
            }
            _ => {
                // New type has no assertions. If no parent is buffering,
                // tear down the builder to avoid a dangling fragment_builder
                // at end_validation. This covers both the case where we
                // popped an old frame (old_element_ref is Some) and the
                // force_start case where no frame was ever pushed
                // (old_element_ref is None but fragment_builder exists).
                if self.assertion_buffer_stack.is_empty() && self.fragment_builder.is_some() {
                    self.fragment_builder.take();
                    self.pending_assertion_frames.clear();
                    if let Some(arena) = self.fragment_arena.as_mut() {
                        arena.reset();
                    }
                }
                return;
            }
        };

        // Get element_ref: prefer the old frame's ref, fall back to saved ref
        let element_ref = old_element_ref.or_else(|| {
            self.validation_stack
                .last()
                .and_then(|ev| ev.assertion_element_ref)
        });

        let Some(element_ref) = element_ref else {
            return;
        };

        // Compute BEFORE pushing: replay needed when no own frame existed AND
        // no parent was buffering (attrs weren't forwarded during attr phase).
        let need_replay = old_element_ref.is_none() && !self.is_buffering_assertions();

        self.assertion_buffer_stack.push(AssertionBufferFrame {
            element_ref,
            complex_type_key: new_ct_key,
            context: ErrorContext::default(),
            location: None,
            propagates_to_outer: false,
        });
        if let Some(ev) = self.validation_stack.last_mut() {
            ev.owns_assertion_buffer = true;
        }

        if need_replay {
            let collected: Vec<_> = match self.validation_stack.last() {
                Some(ev) => ev.collected_attributes.clone(),
                None => return,
            };
            for (ns, name, value) in &collected {
                let local = self.schema_set.name_table.resolve(*name);
                let ns_str = ns
                    .map(|id| self.schema_set.name_table.resolve(id).to_string())
                    .unwrap_or_default();
                let result = self
                    .fragment_builder
                    .as_mut()
                    .map(|b| b.attribute(&local, &ns_str, "", value));
                if let Some(Err(e)) = result {
                    self.abort_assertion_buffer_op("attribute replay", e);
                    return;
                }
            }
        }
    }

    /// Report assertion errors for the outermost frame (current element).
    #[cfg(feature = "xsd11")]
    fn report_assertion_errors(
        &mut self,
        assertion_errors: Vec<ValidationError>,
        ev_state: &mut ElementValidationState,
    ) {
        for err in assertion_errors {
            if !self.continue_run() {
                return;
            }
            self.report_validation_error_to(err, &mut ev_state.error_codes);
            ev_state.validity = SchemaValidity::Invalid;
        }
    }

    /// Report assertion errors for deferred (nested) frames using their stored
    /// path and location instead of the current runtime state.
    ///
    /// Reporting only: a nested element's assertions do not bear on the outer
    /// element's *local* validity (§3.13.4.1 — each element is checked against
    /// its own type's assertions), but its `[validity]` does reach the outer
    /// element through §3.3.5.1 clause 1.1.2; the caller applies that, gated by
    /// [`AssertionBufferFrame::propagates_to_outer`].
    #[cfg(feature = "xsd11")]
    fn report_assertion_errors_deferred(
        &mut self,
        assertion_errors: Vec<ValidationError>,
        context: &ErrorContext,
        location: &Option<SourceLocation>,
    ) {
        for err in assertion_errors {
            if !self.continue_run() {
                return;
            }
            let err = context.attach(err, true);
            let err = match location {
                Some(loc) => err.with_location(loc.clone()),
                None => err,
            };
            self.dispatch_error(err);
        }
    }

    /// Called when a nested asserted element closes and its frame is deferred
    /// (its own element already popped): whether a failure of that frame,
    /// known only when the outermost asserted element closes, still makes the
    /// outermost one invalid. It does when every open element from the
    /// outermost asserted one to the top of the stack — the nested element's
    /// ancestors in between — was strictly assessed: `[validity]` = invalid
    /// passes from child to parent only through strictly assessed elements
    /// (§3.3.5.1 clause 1.1.2); a laxly assessed one is `notKnown` (clause 2)
    /// whatever its children.
    ///
    /// The outermost asserted element owns the bottom frame of
    /// `assertion_buffer_stack`; counting frame owners down from the top of the
    /// validation stack finds it (an owner left over from an aborted buffer
    /// sits further down and is not counted).
    #[cfg(feature = "xsd11")]
    fn nested_failure_reaches_outer(&self) -> bool {
        let live_frames = self.assertion_buffer_stack.len();
        if live_frames == 0 {
            return false;
        }
        let mut owners = 0;
        for ev in self.validation_stack.iter().rev() {
            if !ev.strictly_assessed {
                return false;
            }
            if ev.owns_assertion_buffer {
                owners += 1;
                if owners == live_frames {
                    return true;
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Push API
    // -----------------------------------------------------------------------

    /// Validate an element start event (string-based lookup)
    ///
    /// `local_name` and `namespace_uri` identify the element.
    /// `xsi_type` is the value of `xsi:type` (if present), as a raw QName string.
    /// `xsi_nil` is the value of `xsi:nil` (if present).
    /// `ns_context` is used to resolve the xsi:type QName prefix.
    pub fn validate_element(
        &mut self,
        local_name: &str,
        namespace_uri: &str,
        xsi_type: Option<&str>,
        xsi_nil: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
    ) -> SchemaInfo {
        if !self.continue_run() {
            return SchemaInfo::invalid();
        }
        let name_id = self.schema_set.name_table.add(local_name);
        let ns_id = if namespace_uri.is_empty() {
            None
        } else {
            Some(self.schema_set.name_table.add(namespace_uri))
        };
        self.validate_element_by_id(name_id, ns_id, xsi_type, xsi_nil, ns_context)
    }

    /// Push a skip-wildcard-matched element onto the stack with
    /// `process_contents=Skip`, `validity=NotKnown`, and no content-model
    /// validation. Callers propagate [`ElementStartOutcome::Empty`] on skip.
    fn push_skipped_element(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        ns_context: &NamespaceContextSnapshot,
    ) {
        let mut ev_state = self.acquire_state(local_name, namespace);
        ev_state.ns_context = Some(ns_context.clone());
        ev_state.process_contents = ContentProcessing::Skip;
        ev_state.content_state = ContentValidatorState::Simple;
        ev_state.validity = SchemaValidity::NotKnown;
        self.push_element(ev_state);
        self.advance_constraints_start_element_skipped(local_name, namespace);
        #[cfg(feature = "xsd11")]
        self.detect_assertions_on_element(None, local_name, namespace);
    }

    /// XSD 1.1 dynamic EDC (§3.4.6.4 / cvc-complex-type rule 5): when a
    /// wildcard accepts an element and resolves it to a governing type/decl,
    /// that governing binding must be consistent with any locally declared
    /// element binding for the same QName in the same content model. Returns
    /// `Some(reason)` on violation; the caller should mark the child invalid
    /// and emit a `cvc-complex-type.5` error after push.
    #[cfg(feature = "xsd11")]
    fn dynamic_edc_violation_reason(
        &self,
        matched_via_wildcard: bool,
        local_name: NameId,
        namespace: Option<NameId>,
        governing_type: Option<TypeKey>,
        governing_decl: Option<ElementKey>,
    ) -> Option<String> {
        if !matched_via_wildcard || !self.schema_set.is_xsd11() {
            return None;
        }
        let parent_ct = match self.validation_stack.last().and_then(|p| p.schema_type) {
            Some(TypeKey::Complex(ct_key)) => ct_key,
            _ => return None,
        };
        match crate::schema::edc::check_dynamic_edc(
            self.schema_set,
            self.subst_groups.as_ref(),
            parent_ct,
            (namespace, local_name),
            governing_type,
            governing_decl,
        ) {
            crate::schema::edc::EdcOutcome::Mismatch { reason } => Some(reason),
            _ => None,
        }
    }

    /// Value-free variant of [`validate_element`](Self::validate_element) for
    /// throughput drivers that discard the start-side `SchemaInfo`. Runs the
    /// identical element-start step but skips constructing the returned
    /// `SchemaInfo`. See `PERF_P1_PLAN` §Step 4.
    pub fn validate_element_novalue(
        &mut self,
        local_name: &str,
        namespace_uri: &str,
        xsi_type: Option<&str>,
        xsi_nil: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
    ) {
        if !self.continue_run() {
            return;
        }
        let name_id = self.schema_set.name_table.add(local_name);
        let ns_id = if namespace_uri.is_empty() {
            None
        } else {
            Some(self.schema_set.name_table.add(namespace_uri))
        };
        self.validate_element_by_id_novalue(name_id, ns_id, xsi_type, xsi_nil, ns_context);
    }

    /// Validate an element start event (NameId fast-path), returning the
    /// element's start-side [`SchemaInfo`].
    pub fn validate_element_by_id(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        xsi_type: Option<&str>,
        xsi_nil: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
    ) -> SchemaInfo {
        match self.start_element_by_id(local_name, namespace, xsi_type, xsi_nil, ns_context) {
            ElementStartOutcome::Invalid => SchemaInfo::invalid(),
            ElementStartOutcome::Empty => SchemaInfo::empty(),
            ElementStartOutcome::Start {
                element_decl,
                schema_type,
                validity,
                is_nil,
                content_type,
                type_source,
            } => SchemaInfo {
                element_decl,
                attribute_decl: None,
                schema_type,
                member_type: None,
                validity,
                validation_attempted: ValidationAttempted::None,
                is_default: false,
                is_nil,
                content_type,
                typed_value: None,
                normalized_value: None,
                schema_error_codes: Vec::new(),
                notation: None,
                deferred_by_cta: false,
                type_source,
                #[cfg(feature = "xsd11")]
                cta_selected: false,
                #[cfg(feature = "xsd11")]
                assertion_outcome: None,
            },
        }
    }

    /// Value-free variant of
    /// [`validate_element_by_id`](Self::validate_element_by_id); same
    /// element-start step, no `SchemaInfo` construction.
    pub fn validate_element_by_id_novalue(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        xsi_type: Option<&str>,
        xsi_nil: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
    ) {
        let _ = self.start_element_by_id(local_name, namespace, xsi_type, xsi_nil, ns_context);
    }

    /// The element-start step shared by both entry points above: state-machine
    /// check, parent content-model advance, declaration/type resolution
    /// (xsi:type, wildcards, substitution), state push, and deferred error
    /// emission — everything except assembling the returned `SchemaInfo`.
    /// The element-start step shared by both entry points above: state-machine
    /// check, parent content-model advance, declaration/type resolution
    /// (xsi:type, wildcards, substitution), state push, and deferred error
    /// emission — everything except assembling the returned `SchemaInfo`.
    ///
    /// The body is the numbered sequence of those responsibilities; each step
    /// is a single-responsibility helper below, in the same shape as
    /// [`finish_top_element`](Self::finish_top_element).
    fn start_element_by_id(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        xsi_type: Option<&str>,
        xsi_nil: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
    ) -> ElementStartOutcome {
        // 1. State machine check (and the XSD 1.1 root assertion-source invariant)
        if !self.check_element_start_preconditions() {
            return ElementStartOutcome::Invalid;
        }

        // 2. If not root: advance parent's content model
        let advance = match self.advance_parent_content_model(local_name, namespace, ns_context) {
            ParentAdvanceOutcome::Settled(outcome) => return outcome,
            ParentAdvanceOutcome::Advanced(advance) => advance,
        };
        if !self.continue_run() {
            return ElementStartOutcome::Invalid;
        }

        // 2b. A validation root: the run's mode (Structures §5.2) decides how
        //     it is assessed. Every other element: as its parent decided.
        let root_mode = if self.validation_stack.is_empty() {
            self.validation_root
        } else {
            ValidationRoot::Declaration
        };
        let mut stipulated_name_mismatch = false;
        let governing = match root_mode {
            // Type-driven validation: no governing element declaration.
            ValidationRoot::Type(stipulated) => {
                return self.start_root_stipulated_type_element(
                    stipulated, local_name, namespace, xsi_type, ns_context,
                );
            }
            // Element-driven validation: the stipulated declaration governs.
            ValidationRoot::Element(stipulated) => {
                let Some((elem_key, name_matches)) =
                    self.stipulated_element(stipulated, local_name, namespace)
                else {
                    self.fail_operational(
                        "validation-root-unknown",
                        "The element declaration stipulated for the validation root is not \
                         part of this run's schema set"
                            .to_string(),
                    );
                    return ElementStartOutcome::Invalid;
                };
                stipulated_name_mismatch = !name_matches;
                GoverningDeclaration {
                    element_key: Some(elem_key),
                    matched_type: None,
                    process_contents: ContentProcessing::Strict,
                    #[cfg(feature = "xsd11")]
                    matched_via_wildcard: false,
                }
            }
            // 3. Look up element declaration: prefer content model match, fall
            //    back to global
            ValidationRoot::Declaration | ValidationRoot::Lax => {
                self.resolve_governing_declaration(local_name, namespace, advance.match_info)
            }
        };

        // 4. No governing element declaration: a wildcard-matched child, root
        //    xsi:type promotion, or an undeclared element (skip/lax/strict).
        let Some(elem_key) = governing.element_key else {
            if advance.content_model_accepted {
                return self.start_wildcard_matched_child(
                    local_name, namespace, xsi_type, ns_context, governing,
                );
            }
            // Root xsi:type promotion: at the document root with no governing
            // element declaration, an `xsi:type` attribute may itself supply
            // the governing type. XSD 1.0 §5.2 / 1.1 §5.2.2 — the element is
            // laxly assessed against the xsi:type-supplied type. This mirrors
            // PATH B2/B3 (which handles the wildcard-child case) and must run
            // *before* the unconditional cvc-elt.1 fall-through.
            if self.validation_stack.is_empty() {
                // Lax wildcard validation (§5.2): an undeclared root is laxly
                // assessed instead of being reported.
                let lax = root_mode == ValidationRoot::Lax;
                if let Some(outcome) = self
                    .start_root_xsi_type_element(local_name, namespace, xsi_type, ns_context, lax)
                {
                    return outcome;
                }
                if lax {
                    return self.start_undeclared_element(
                        local_name,
                        namespace,
                        ns_context,
                        ContentProcessing::Lax,
                    );
                }
            }
            return self.start_undeclared_element(
                local_name,
                namespace,
                ns_context,
                governing.process_contents,
            );
        };

        // 5-7. Governing type from the declaration: declared type, xsi:type
        //      override, abstract checks, xsi:nil — diagnostics deferred.
        let mut xsi_type_deferred_errors = Vec::new();
        let binding = self.resolve_declared_element_type(
            elem_key,
            xsi_type,
            xsi_nil,
            ns_context,
            &mut xsi_type_deferred_errors,
        );

        // 8. Initialize content model and determine ContentType
        let (content_state, content_type) = self.init_content_model(binding.type_key);

        // §3.4.6.4 dynamic EDC: when this element was matched by a wildcard
        // (not directly by an element particle), the resolved governing decl
        // and effective type must be consistent with any QName-equal local
        // element declaration in the parent CT.
        #[cfg(feature = "xsd11")]
        let edc_violation_a = self.dynamic_edc_violation_reason(
            governing.matched_via_wildcard,
            local_name,
            namespace,
            binding.type_key,
            Some(elem_key),
        );
        #[cfg(feature = "xsd11")]
        let edc_invalid = edc_violation_a.is_some();
        #[cfg(not(feature = "xsd11"))]
        let edc_invalid = false;
        // Element Locally Valid (Element) (§3.3.4.3) clause 1 for a stipulated
        // declaration of another name: invalid from the start.
        let start_invalid = edc_invalid || stipulated_name_mismatch;

        // 9. Push ElementValidationState
        let mut ev_state = self.acquire_state(local_name, namespace);
        ev_state.ns_context = Some(ns_context.clone());
        ev_state.content_state = content_state;
        init_declared_element_state(
            &mut ev_state,
            elem_key,
            binding,
            governing.process_contents,
            content_type,
            start_invalid,
        );
        self.push_element(ev_state);
        if stipulated_name_mismatch {
            let elem_name = self.schema_set.name_table.resolve(local_name).to_string();
            self.report_error(
                "cvc-elt.1",
                format!(
                    "Element '{}' does not have the name of the element declaration stipulated \
                     for the validation root",
                    elem_name
                ),
            );
        }
        #[cfg(feature = "xsd11")]
        if let Some(reason) = edc_violation_a.as_ref() {
            let elem_name = self.schema_set.name_table.resolve(local_name).to_string();
            self.report_error(
                "cvc-complex-type.5",
                format!(
                    "Element '{}' matched a wildcard but its governing element declaration \
                     is inconsistent with the local element declaration in the parent's \
                     content model: {}",
                    elem_name, reason,
                ),
            );
        }

        // Emit deferred xsi:type errors now that the child is on the stack
        self.emit_deferred_xsi_type_errors(xsi_type_deferred_errors);

        // Report deferred element-start errors AFTER push so codes land on the child
        self.report_declared_element_start_errors(local_name, elem_key, binding);

        self.advance_constraints_start_element(local_name, namespace, Some(elem_key));

        // 9b. Assertion detection hook (XSD 1.1)
        #[cfg(feature = "xsd11")]
        self.detect_assertions_on_element(binding.type_key, local_name, namespace);

        // 10. Return the start outcome
        ElementStartOutcome::Start {
            element_decl: Some(elem_key),
            schema_type: binding.type_key,
            validity: binding.state_validity(start_invalid),
            is_nil: binding.is_nil,
            content_type: Some(content_type),
            type_source: Some(binding.type_source),
        }
    }

    /// Element-start step 1: entry preconditions — a terminal operational
    /// failure short-circuits the run, the state machine must allow an element
    /// start, and (XSD 1.1) the root element asserts the `PROCESS_ASSERTIONS` ↔
    /// [`AssertionSource`] invariant. Returns `false` when the caller must
    /// abandon the start with [`ElementStartOutcome::Invalid`].
    fn check_element_start_preconditions(&mut self) -> bool {
        if !self.continue_run() {
            return false;
        }
        // The parent's attributes are behind; none of them is this element's.
        #[cfg(feature = "xsd11")]
        self.cta_unlisted_attributes.clear();
        // 1. State machine check
        if !self.current_state.can_start_element() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "Element start not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return false;
        }

        // 1b. Root element: verify PROCESS_ASSERTIONS ↔ AssertionSource consistency
        #[cfg(feature = "xsd11")]
        if self.validation_stack.is_empty() {
            let has_flag = self.flags.contains(ValidationFlags::PROCESS_ASSERTIONS);
            let is_fragment = self.assertion_source == AssertionSource::FragmentBuffer;
            assert!(
                has_flag == is_fragment,
                "PROCESS_ASSERTIONS flag and AssertionSource are inconsistent: \
                 flag={has_flag}, source={:?}. Call set_assertion_source() before validation.",
                self.assertion_source,
            );
        }
        true
    }

    /// Element-start step 2: advance the parent's content model for this child
    /// (§3.4.4 cvc-complex-type.2.4).
    ///
    /// Handles the parent states that end assessment here — a skipped parent
    /// (the child is pushed as skipped), a nilled parent (cvc-elt.3.2.1), and a
    /// content model that could not be advanced within its execution limits
    /// (`validation-resource-limit`, §8.4) — and otherwise reports the matching
    /// content-model term. At the document root there is no parent, so no match
    /// is reported.
    fn advance_parent_content_model(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        ns_context: &NamespaceContextSnapshot,
    ) -> ParentAdvanceOutcome {
        let mut match_info: Option<super::content::ElementMatchInfo> = None;
        let mut content_model_accepted = false;
        let mut content_model_error = None;
        let mut nil_error: Option<String> = None;
        let mut resource_failure: Option<(
            crate::compiler::ContentModelLimitExceeded,
            NameId,
            Option<TypeKey>,
        )> = None;
        if let Some(parent) = self.validation_stack.last_mut() {
            if parent.process_contents == ContentProcessing::Skip {
                // Skipped element: don't validate content model, push as skip, return
                parent.has_element_children = true;
                self.push_skipped_element(local_name, namespace, ns_context);
                return ParentAdvanceOutcome::Settled(ElementStartOutcome::Empty);
            } else if parent.is_nil {
                let parent_name = self
                    .schema_set
                    .name_table
                    .resolve(parent.local_name)
                    .to_string();
                nil_error = Some(format!(
                    "Element '{}' is nilled (xsi:nil='true') but has child element content",
                    parent_name,
                ));
            } else {
                parent.has_element_children = true;
                // Derive wildcard target namespace from the parent's schema
                // type, not the instance element namespace. For unqualified
                // local elements parent.namespace may be None while the
                // wildcard's ##targetNamespace should resolve to the schema
                // document's target namespace (XSD spec §3.10.4).
                let wildcard_target_ns = match parent.schema_type {
                    Some(TypeKey::Complex(ct_key)) => {
                        self.schema_set.arenas.complex_types[ct_key].target_namespace
                    }
                    _ => parent.namespace,
                };
                match parent.content_state.try_advance_element(
                    local_name,
                    namespace,
                    wildcard_target_ns,
                    self.schema_set.xsd_version,
                    self.subst_groups.as_ref(),
                ) {
                    Ok(Some(info)) => {
                        match_info = Some(info);
                        content_model_accepted = true;
                    }
                    Ok(None) => {
                        // cvc-complex-type.2.4 is a constraint on the *parent*
                        // (Structures §3.4.4, Element Locally Valid (Complex
                        // Type)): a child that the content model rejects makes
                        // the parent invalid, not merely error-annotated.
                        parent.validity = SchemaValidity::Invalid;
                        let elem_name = self.schema_set.name_table.resolve(local_name);
                        content_model_error = Some(format!(
                            "Element '{}' is not allowed at this position in the content model",
                            elem_name,
                        ));
                    }
                    Err(limit) => {
                        resource_failure = Some((limit, parent.local_name, parent.schema_type));
                    }
                }
            }
        }
        if let Some((limit, parent_name, parent_type)) = resource_failure {
            // Not a verdict: the counted content model could not be advanced
            // within the execution limits. Abort the run (§8.4).
            let parent_name = self.schema_set.name_table.resolve(parent_name).to_string();
            let type_label = match parent_type {
                Some(TypeKey::Complex(ct)) => self.complex_type_label(ct),
                _ => "(no complex type)".to_string(),
            };
            self.fail_operational(
                "validation-resource-limit",
                format!(
                    "Content model of element '{parent_name}' (type {type_label}) exceeded an \
                     execution limit — {limit}; assessment aborted"
                ),
            );
            return ParentAdvanceOutcome::Settled(ElementStartOutcome::Invalid);
        }
        if let Some(msg) = nil_error {
            // The nilled *parent* (still the stack top) has an element child:
            // §3.3.4.3 clause 3.2.3.1, "E has no character or element
            // information item [children]".
            self.invalidate_current();
            self.report_native_error("cvc-elt.3.2.1", msg, errors::NativeCause::NilWithContent);
        }
        if let Some(msg) = content_model_error {
            let names = &self.schema_set.name_table;
            let found = ExpandedName::new(
                namespace.map(|n| names.resolve_ref(n)),
                names.resolve_ref(local_name),
            );
            let mut err = errors::error("cvc-complex-type.2.4", msg, self.current_location.clone())
                .with_native_cause(errors::NativeCause::UnexpectedChild { found });
            if let Some(key) = self.validation_stack.last().and_then(|s| s.schema_type) {
                err = err.with_type_context(key, self.schema_set);
            }
            self.report_validation_error(err);
        }
        ParentAdvanceOutcome::Advanced(ParentAdvance {
            match_info,
            content_model_accepted,
        })
    }

    /// Element-start step 3: resolve the governing element declaration and the
    /// effective `{process contents}` for this child — the content-model match
    /// first, then the global element table (§3.10.4 cvc-wildcard).
    fn resolve_governing_declaration(
        &self,
        local_name: NameId,
        namespace: Option<NameId>,
        match_info: Option<super::content::ElementMatchInfo>,
    ) -> GoverningDeclaration {
        let matched_elem_key = match_info.and_then(|i| i.element_key);
        let matched_type = match_info.and_then(|i| i.resolved_type);

        // Was this element matched via a content-model wildcard (or open content)?
        // Set when the matched particle was a wildcard, regardless of strict/lax/skip.
        // §3.4.6.4 dynamic EDC fires only on wildcard matches.
        #[cfg(feature = "xsd11")]
        let matched_via_wildcard = match_info
            .as_ref()
            .and_then(|i| i.process_contents)
            .is_some();

        // Determine process_contents before element_key lookup: a skip wildcard
        // must suppress the global declaration lookup (§3.10.4 cvc-wildcard).
        let process_contents = match_info
            .and_then(|i| i.process_contents)
            .map(|pc| match pc {
                TypesProcessContents::Strict => ContentProcessing::Strict,
                TypesProcessContents::Lax => ContentProcessing::Lax,
                TypesProcessContents::Skip => ContentProcessing::Skip,
            })
            .unwrap_or_else(|| {
                self.validation_stack
                    .last()
                    .map(|p| p.process_contents)
                    .unwrap_or(ContentProcessing::Strict)
            });

        // If the content model provided a resolved type for a local element,
        // don't fall back to a global element with the same QName (it may have
        // a different type).
        let element_key = if matched_type.is_some() || process_contents == ContentProcessing::Skip {
            matched_elem_key
        } else {
            matched_elem_key.or_else(|| self.schema_set.lookup_element(namespace, local_name))
        };
        GoverningDeclaration {
            element_key,
            matched_type,
            process_contents,
            #[cfg(feature = "xsd11")]
            matched_via_wildcard,
        }
    }

    /// Element-start step 4a: a child the parent's content model accepted for
    /// which no global element declaration exists (a wildcard or open-content
    /// match, or a local term whose type the model resolved). Pushes the child
    /// and returns its start outcome.
    fn start_wildcard_matched_child(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        xsi_type: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
        governing: GoverningDeclaration,
    ) -> ElementStartOutcome {
        let process_contents = governing.process_contents;
        if process_contents == ContentProcessing::Skip {
            self.push_skipped_element(local_name, namespace, ns_context);
            return ElementStartOutcome::Empty;
        }
        // Content model accepted this element (wildcard in content model)
        // but no global declaration exists. Structures §3.3.1: E is ·nilled·
        // "with respect to some element declaration D" when "1 E has xsi:nil =
        // true. 2 D.{nillable} = true" — so an element governed by no
        // declaration the runtime holds is never nilled: its `xsi:nil` is an
        // attribute of the built-in declaration (`xs:boolean`), and its
        // content is checked against its governing type, if any. That holds
        // for PATH B2 / B3 (a wildcard's element) and for PATH B1 too: a local
        // element term the content model resolved with a type but without its
        // declaration's key, whose {nillable} is therefore unknown (a
        // defensive path — no test and no W3C case reaches it).
        let is_nil = false;
        let mut ev_state = self.acquire_state(local_name, namespace);
        ev_state.ns_context = Some(ns_context.clone());
        ev_state.validity = SchemaValidity::Valid;
        ev_state.process_contents = process_contents;
        ev_state.is_nil = is_nil;

        let mut wildcard_xsi_type_errors = Vec::new();
        self.apply_wildcard_child_type(
            &mut ev_state,
            governing.matched_type,
            xsi_type,
            process_contents,
            ns_context,
            &mut wildcard_xsi_type_errors,
        );

        ev_state.strictly_assessed = (ev_state.element_decl.is_some()
            || ev_state.schema_type.is_some())
            && ev_state.process_contents != ContentProcessing::Skip;
        // Laxly assessed (no governing declaration or type, Schema-Validity
        // Assessment (Element) §3.3.4.6 clause 3): §3.3.5.1 `[validity]` "2
        // otherwise notKnown", whatever its attributes and children — an
        // `xsi:type` that resolved to no type included (no ·instance-specified
        // type definition·, §3.3.4.1 clause 3). Under a strict wildcard the
        // element is an error (`cvc-elt.1`) and stays invalid.
        if !ev_state.strictly_assessed && process_contents == ContentProcessing::Lax {
            ev_state.validity = SchemaValidity::NotKnown;
        }
        // §3.4.6.4 dynamic EDC for the no-element-key wildcard branch:
        // even without a governing global declaration, an xsi:type may
        // supply a governing type whose binding must agree with any
        // QName-equal local element declaration in the parent CT.
        #[cfg(feature = "xsd11")]
        let edc_violation_b = self.dynamic_edc_violation_reason(
            governing.matched_via_wildcard,
            local_name,
            namespace,
            ev_state.schema_type,
            None,
        );
        #[cfg(feature = "xsd11")]
        if edc_violation_b.is_some() {
            ev_state.validity = SchemaValidity::Invalid;
        }
        let schema_type = ev_state.schema_type;
        let content_type = ev_state.content_type;
        let validity = ev_state.validity;
        let type_source = ev_state.type_source;
        let needs_undeclared_error =
            process_contents == ContentProcessing::Strict && schema_type.is_none();
        self.push_element(ev_state);
        #[cfg(feature = "xsd11")]
        if let Some(reason) = edc_violation_b {
            let elem_name = self.schema_set.name_table.resolve(local_name).to_string();
            self.report_error(
                "cvc-complex-type.5",
                format!(
                    "Element '{}' matched a wildcard but its governing type is \
                     inconsistent with the local element declaration in the \
                     parent's content model: {}",
                    elem_name, reason,
                ),
            );
        }
        // Emit deferred xsi:type errors now that the child is on the stack
        self.emit_deferred_xsi_type_errors(wildcard_xsi_type_errors);
        if needs_undeclared_error {
            let elem_name = self.schema_set.name_table.resolve(local_name);
            self.report_error(
                "cvc-elt.1",
                format!("Element '{}' is not declared", elem_name),
            );
        }
        self.advance_constraints_start_element(local_name, namespace, None);
        #[cfg(feature = "xsd11")]
        self.detect_assertions_on_element(schema_type, local_name, namespace);
        ElementStartOutcome::Start {
            element_decl: None,
            schema_type,
            validity,
            is_nil,
            content_type,
            type_source,
        }
    }

    /// Element-start step 4a (i): the governing type of a wildcard-matched
    /// child that has no element declaration.
    ///
    /// PATH B1 — the content model resolved a type for the local element, which
    /// an `xsi:type` may still override; PATH B2 — no matched type, but
    /// `xsi:type` supplies a governing type; PATH B3 — neither, so the child is
    /// laxly assessed via `xs:anyType`. Writes the outcome (type, type source,
    /// content model, validity) into `ev_state`; `xsi:type` diagnostics are
    /// collected into `deferred_errors` for emission after the push.
    fn apply_wildcard_child_type(
        &mut self,
        ev_state: &mut ElementValidationState,
        matched_type: Option<TypeKey>,
        xsi_type: Option<&str>,
        process_contents: ContentProcessing,
        ns_context: &NamespaceContextSnapshot,
        deferred_errors: &mut Vec<ValidationError>,
    ) {
        if let Some(mut type_key) = matched_type {
            // PATH B1: xsi:type override for local elements with resolved type
            let mut b1_type_source = TypeSource::Declaration;
            if let Some(xsi_type_str) = xsi_type {
                match self.resolve_xsi_type(
                    xsi_type_str,
                    Some(type_key),
                    DerivationSet::empty(),
                    ns_context,
                    deferred_errors,
                ) {
                    XsiTypeOutcome::Applied(overridden) => {
                        type_key = overridden;
                        b1_type_source = TypeSource::XsiType;
                    }
                    XsiTypeOutcome::Unresolved | XsiTypeOutcome::InvalidDerivation => {
                        ev_state.validity = SchemaValidity::Invalid;
                        // keep original type_key
                    }
                }
            }
            // Local element with resolved type — initialize content model
            let (content_state, content_type) = self.init_content_model(Some(type_key));
            ev_state.schema_type = Some(type_key);
            ev_state.type_source = Some(b1_type_source);
            ev_state.content_state = content_state;
            ev_state.content_type = Some(content_type);
        } else {
            // PATH B2/B3: No declaration and no matched type.
            // Try xsi:type first — it can supply a governing type even
            // without a declaration.
            if let Some(xsi_type_str) = xsi_type {
                match self.resolve_xsi_type(
                    xsi_type_str,
                    None,
                    DerivationSet::empty(),
                    ns_context,
                    deferred_errors,
                ) {
                    XsiTypeOutcome::Applied(overridden) => {
                        let (content_state, content_type) =
                            self.init_content_model(Some(overridden));
                        ev_state.schema_type = Some(overridden);
                        ev_state.type_source = Some(TypeSource::XsiType);
                        ev_state.content_state = content_state;
                        ev_state.content_type = Some(content_type);
                    }
                    XsiTypeOutcome::Unresolved | XsiTypeOutcome::InvalidDerivation => {
                        // No governing type — lax assessment
                        ev_state.validity = SchemaValidity::Invalid;
                        let (content_state, content_type) = self.lax_assessment_content_model();
                        ev_state.content_state = content_state;
                        ev_state.content_type = Some(content_type);
                        // schema_type stays None (no governing type)
                    }
                }
            } else {
                // PATH B3: No governing declaration/type — lax assessment via xs:anyType
                let (content_state, content_type) = self.lax_assessment_content_model();
                ev_state.content_state = content_state;
                ev_state.content_type = Some(content_type);
                // schema_type stays None
            }
            // Strict wildcard with no global declaration and no governing
            // type from xsi:type → cvc-elt.1.  Checked AFTER xsi:type so
            // that a valid xsi:type can still supply assessment.
            // Error deferred until after push so it lands on the child.
            if process_contents == ContentProcessing::Strict && ev_state.schema_type.is_none() {
                ev_state.validity = SchemaValidity::Invalid;
            }
        }
    }

    /// Element-start step 4b: root `xsi:type` promotion (XSD 1.0 §5.2 / 1.1
    /// §5.2.2). At the document root with no governing element declaration, an
    /// `xsi:type` attribute may itself supply the governing type; the element is
    /// then laxly assessed against it. Returns `None` when there is no
    /// `xsi:type` and the caller must fall through to the undeclared-element
    /// paths.
    ///
    /// With `lax` (lax wildcard validation, [`ValidationRoot::Lax`]) an
    /// `xsi:type` that resolves to no usable type is reported but the root is
    /// not: it is laxly assessed, as an element a lax wildcard matched (PATH
    /// B2), without `cvc-elt.1` — `[validity]` notKnown. The root is never
    /// ·nilled· (no declaration, §3.3.1).
    fn start_root_xsi_type_element(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        xsi_type: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
        lax: bool,
    ) -> Option<ElementStartOutcome> {
        let xsi_type_str = xsi_type?;
        let mut deferred = Vec::new();
        match self.resolve_xsi_type(
            xsi_type_str,
            None,
            DerivationSet::empty(),
            ns_context,
            &mut deferred,
        ) {
            XsiTypeOutcome::Applied(type_key) => {
                // No governing element declaration, so never ·nilled· (§3.3.1:
                // "with respect to some element declaration D … D.{nillable} =
                // true"): `xsi:nil` is only an attribute, the content is
                // checked against the type.
                let mut ev_state = self.acquire_state(local_name, namespace);
                ev_state.ns_context = Some(ns_context.clone());
                let (content_state, content_type) = self.init_content_model(Some(type_key));
                ev_state.schema_type = Some(type_key);
                ev_state.type_source = Some(TypeSource::XsiType);
                ev_state.content_state = content_state;
                ev_state.content_type = Some(content_type);
                ev_state.is_nil = false;
                ev_state.validity = SchemaValidity::Valid;
                ev_state.process_contents = ContentProcessing::Strict;
                ev_state.strictly_assessed = true;
                self.push_element(ev_state);
                self.advance_constraints_start_element(local_name, namespace, None);
                #[cfg(feature = "xsd11")]
                self.detect_assertions_on_element(Some(type_key), local_name, namespace);
                Some(ElementStartOutcome::Start {
                    element_decl: None,
                    schema_type: Some(type_key),
                    validity: SchemaValidity::Valid,
                    is_nil: false,
                    content_type: Some(content_type),
                    type_source: Some(TypeSource::XsiType),
                })
            }
            XsiTypeOutcome::Unresolved | XsiTypeOutcome::InvalidDerivation => {
                // xsi:type failed to resolve at the root — fall
                // through to the Strict arm below for cvc-elt.1.
                // Emit the deferred xsi:type diagnostic in
                // addition, after push so attribution lands on
                // the child.
                let mut ev_state = self.acquire_state(local_name, namespace);
                ev_state.ns_context = Some(ns_context.clone());
                // Lax: no ·instance-specified type definition· (§3.3.4.1
                // clause 3), so laxly assessed — notKnown (§3.3.5.1 clause 2).
                ev_state.validity = if lax {
                    SchemaValidity::NotKnown
                } else {
                    SchemaValidity::Invalid
                };
                if lax {
                    ev_state.process_contents = ContentProcessing::Lax;
                }
                let (content_state, content_type) = self.lax_assessment_content_model();
                ev_state.content_state = content_state;
                ev_state.content_type = Some(content_type);
                self.push_element(ev_state);
                if !lax {
                    let elem_name = self.schema_set.name_table.resolve(local_name).to_string();
                    self.report_error(
                        "cvc-elt.1",
                        format!("Element '{}' is not declared", elem_name),
                    );
                }
                self.emit_deferred_xsi_type_errors(deferred);
                self.advance_constraints_start_element(local_name, namespace, None);
                #[cfg(feature = "xsd11")]
                self.detect_assertions_on_element(None, local_name, namespace);
                Some(if lax {
                    ElementStartOutcome::Empty
                } else {
                    ElementStartOutcome::Invalid
                })
            }
        }
    }

    /// Element-start step 2b (element-driven validation,
    /// [`ValidationRoot::Element`]): the declaration stipulated for the root —
    /// the one a reference entry resolves to — and whether its expanded name
    /// is the element's. `None` when the run's schema set does not hold it.
    fn stipulated_element(
        &self,
        stipulated: ElementKey,
        local_name: NameId,
        namespace: Option<NameId>,
    ) -> Option<(ElementKey, bool)> {
        let elements = &self.schema_set.arenas.elements;
        let mut key = stipulated;
        let mut decl = elements.get(key)?;
        if let Some(target) = decl.resolved_ref {
            key = target;
            decl = elements.get(key)?;
        }
        Some((
            key,
            decl.name == Some(local_name) && decl.target_namespace == namespace,
        ))
    }

    /// Element-start step 2b (type-driven validation,
    /// [`ValidationRoot::Type`]): a validation root whose governing type the
    /// run stipulated. Structures §3.3.4.6: no ·governing element
    /// declaration· (clause 4.2), and the governing type definition is "1 An
    /// ·instance-specified type definition· which ·overrides· a type
    /// definition stipulated by the processor", else "2 A type definition
    /// stipulated by the processor". The `xsi:type` check is the declared
    /// element's (cvc-elt.4.1 – 4.3) with the stipulated type in the declared
    /// type's place and no {disallowed substitutions}; an `xsi:type` that does
    /// not override is reported and the stipulated type stays governing. The
    /// root is strictly assessed (clause 1.2) and never ·nilled· (there is no
    /// declaration, §3.3.1).
    fn start_root_stipulated_type_element(
        &mut self,
        stipulated: TypeKey,
        local_name: NameId,
        namespace: Option<NameId>,
        xsi_type: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
    ) -> ElementStartOutcome {
        let arenas = &self.schema_set.arenas;
        let held = match stipulated {
            TypeKey::Complex(k) => arenas.complex_types.get(k).is_some(),
            TypeKey::Simple(k) => arenas.simple_types.get(k).is_some(),
        };
        if !held {
            self.fail_operational(
                "validation-root-unknown",
                "The type definition stipulated for the validation root is not part of this \
                 run's schema set"
                    .to_string(),
            );
            return ElementStartOutcome::Invalid;
        }

        let mut deferred = Vec::new();
        let mut type_key = stipulated;
        let mut type_source = TypeSource::Stipulated;
        let mut xsi_type_invalid = false;
        if let Some(xsi_type_str) = xsi_type {
            match self.resolve_xsi_type(
                xsi_type_str,
                Some(stipulated),
                DerivationSet::empty(),
                ns_context,
                &mut deferred,
            ) {
                XsiTypeOutcome::Applied(overridden) => {
                    type_key = overridden;
                    type_source = TypeSource::XsiType;
                }
                XsiTypeOutcome::Unresolved | XsiTypeOutcome::InvalidDerivation => {
                    xsi_type_invalid = true;
                }
            }
        }
        // Element Locally Valid (Type) (§3.3.4.4) clause 2: "If T is a complex
        // type definition, then T.{abstract} = false."
        let abstract_ct_info = match type_key {
            TypeKey::Complex(k) if !xsi_type_invalid => self
                .schema_set
                .arenas
                .complex_types
                .get(k)
                .filter(|ct| ct.is_abstract)
                .map(|ct| (ct.name, ct.target_namespace)),
            _ => None,
        };

        let (content_state, content_type) = self.init_content_model(Some(type_key));
        let validity = if xsi_type_invalid || abstract_ct_info.is_some() {
            SchemaValidity::Invalid
        } else {
            SchemaValidity::Valid
        };
        let mut ev_state = self.acquire_state(local_name, namespace);
        ev_state.ns_context = Some(ns_context.clone());
        ev_state.schema_type = Some(type_key);
        ev_state.type_source = Some(type_source);
        ev_state.content_state = content_state;
        ev_state.content_type = Some(content_type);
        ev_state.validity = validity;
        ev_state.process_contents = ContentProcessing::Strict;
        ev_state.strictly_assessed = true;
        self.push_element(ev_state);
        // Diagnostics after the push, so they land on the root.
        self.emit_deferred_xsi_type_errors(deferred);
        if let Some((ct_name, ct_ns)) = abstract_ct_info {
            let type_name =
                crate::schema::derivation::format_type_name(self.schema_set, ct_name, ct_ns);
            self.report_error(
                "cvc-type.2",
                format!(
                    "Type '{}' is abstract and cannot be used to validate an element",
                    type_name
                ),
            );
        }
        self.advance_constraints_start_element(local_name, namespace, None);
        #[cfg(feature = "xsd11")]
        self.detect_assertions_on_element(Some(type_key), local_name, namespace);
        ElementStartOutcome::Start {
            element_decl: None,
            schema_type: Some(type_key),
            validity,
            is_nil: false,
            content_type: Some(content_type),
            type_source: Some(type_source),
        }
    }

    /// Element-start step 4c: an element with no governing declaration that the
    /// parent's content model did not accept through a wildcard — skip, lax, or
    /// strict assessment (cvc-elt.1 under strict).
    fn start_undeclared_element(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        ns_context: &NamespaceContextSnapshot,
        process_contents: ContentProcessing,
    ) -> ElementStartOutcome {
        match process_contents {
            ContentProcessing::Skip => {
                // Skip validation entirely
                let mut ev_state = self.acquire_state(local_name, namespace);
                ev_state.ns_context = Some(ns_context.clone());
                ev_state.process_contents = ContentProcessing::Skip;
                ev_state.content_state = ContentValidatorState::Simple; // accept anything
                ev_state.validity = SchemaValidity::NotKnown;
                self.push_element(ev_state);
                self.advance_constraints_start_element(local_name, namespace, None);
                #[cfg(feature = "xsd11")]
                self.detect_assertions_on_element(None, local_name, namespace);
                ElementStartOutcome::Empty
            }
            ContentProcessing::Lax => {
                // Lax: no declaration found — lax assessment via xs:anyType
                let mut ev_state = self.acquire_state(local_name, namespace);
                ev_state.ns_context = Some(ns_context.clone());
                ev_state.process_contents = ContentProcessing::Lax;
                let (content_state, content_type) = self.lax_assessment_content_model();
                ev_state.content_state = content_state;
                ev_state.content_type = Some(content_type);
                // schema_type stays None — no governing type
                ev_state.validity = SchemaValidity::NotKnown;
                self.push_element(ev_state);
                self.advance_constraints_start_element(local_name, namespace, None);
                #[cfg(feature = "xsd11")]
                self.detect_assertions_on_element(None, local_name, namespace);
                ElementStartOutcome::Empty
            }
            ContentProcessing::Strict => {
                let mut ev_state = self.acquire_state(local_name, namespace);
                ev_state.ns_context = Some(ns_context.clone());
                ev_state.validity = SchemaValidity::Invalid;
                // Lax assessment for content (same PSVI as lax when no declaration found)
                let (content_state, content_type) = self.lax_assessment_content_model();
                ev_state.content_state = content_state;
                ev_state.content_type = Some(content_type);
                // schema_type stays None
                self.push_element(ev_state);
                // Report error AFTER push so code lands on the child, not parent
                let elem_name = self.schema_set.name_table.resolve(local_name);
                self.report_native_error(
                    "cvc-elt.1",
                    format!("Element '{}' is not declared", elem_name),
                    errors::NativeCause::UndeclaredElement,
                );
                self.advance_constraints_start_element(local_name, namespace, None);
                #[cfg(feature = "xsd11")]
                self.detect_assertions_on_element(None, local_name, namespace);
                ElementStartOutcome::Invalid
            }
        }
    }

    /// Element-start steps 5-7: the governing type of a declared element.
    ///
    /// Reads the declaration's resolved type, applies any `xsi:type` override
    /// (masked to the element-relevant `{block}` bits), and collects the
    /// verdicts whose diagnostics are deferred until after the push:
    /// cvc-elt.2 (abstract declaration), §3.3.4.4 cvc-type clause 2 (abstract
    /// type — deferred again to CTA under XSD 1.1), cvc-elt.3.1 (`xsi:nil` on a
    /// non-nillable declaration) and §src-resolve. Emits nothing itself;
    /// `xsi:type` errors go to `deferred_errors`.
    fn resolve_declared_element_type(
        &self,
        elem_key: ElementKey,
        xsi_type: Option<&str>,
        xsi_nil: Option<&str>,
        ns_context: &NamespaceContextSnapshot,
        deferred_errors: &mut Vec<ValidationError>,
    ) -> DeclaredElementBinding {
        let elem_data = &self.schema_set.arenas.elements[elem_key];

        // Check abstract (deferred until after push)
        let is_abstract = elem_data.is_abstract;

        // §src-resolve: when an explicit `type` attribute could not be
        // resolved at compile time, the XSD 1.0 lazy path leaves a
        // `deferred_type_error` on the declaration. Surface it on first
        // selection rather than silently falling back to `xs:anyType`.
        let has_deferred_type_error = elem_data.deferred_type_error.is_some();

        // 5. Resolve type from element declaration
        let mut type_key = elem_data.resolved_type;

        // 6. xsi:type override
        // Errors are deferred and emitted after push so they land on the child element.
        let (effective_block, _) = crate::compiler::substitution::effective_element_constraints(
            self.schema_set,
            elem_data,
        );
        // Mask to element-relevant bits only (extension, restriction, substitution)
        // to avoid spuriously blocking list/union derivation steps.
        let effective_block = effective_block.element_block_mask();
        let mut xsi_type_invalid = false;
        let mut type_source = TypeSource::Declaration;
        if let Some(xsi_type_str) = xsi_type {
            // Default to anyType when no explicit type is declared (cvc-elt.4.3 still applies)
            let declared_for_xsi =
                type_key.or(Some(TypeKey::Complex(self.schema_set.any_type_key())));
            match self.resolve_xsi_type(
                xsi_type_str,
                declared_for_xsi,
                effective_block,
                ns_context,
                deferred_errors,
            ) {
                XsiTypeOutcome::Applied(overridden) => {
                    type_key = Some(overridden);
                    type_source = TypeSource::XsiType;
                }
                XsiTypeOutcome::Unresolved | XsiTypeOutcome::InvalidDerivation => {
                    xsi_type_invalid = true;
                }
            }
        }

        // §3.3.4.4 cvc-type clause 2: if T is a complex type definition, T.{abstract} must be false.
        // Hoist the complex-type fetch once so the cvc-type.2 error path below can
        // reuse the name/target_namespace without re-resolving the arena entry.
        let abstract_ct_info: Option<(Option<NameId>, Option<NameId>)> = if !xsi_type_invalid {
            if let Some(TypeKey::Complex(k)) = type_key {
                self.schema_set
                    .arenas
                    .complex_types
                    .get(k)
                    .filter(|ct| ct.is_abstract)
                    .map(|ct| (ct.name, ct.target_namespace))
            } else {
                None
            }
        } else {
            None
        };
        #[cfg(feature = "xsd11")]
        let has_type_alternatives = !self.schema_set.arenas.elements[elem_key]
            .alternatives
            .is_empty();
        // §3.12.4 (XSD 1.1): when an element declaration carries type
        // alternatives, the *governing* type is not known until after attribute
        // processing (conditional type assignment). The declared type may be
        // abstract (e.g. OpenDRIVE `t_junction`), but a concrete alternative
        // is always selected. Defer the `cvc-type.2` abstract-type check to
        // `end_of_attributes_inner`, which re-checks the *final* governing type.
        // This avoids a false-positive on every CTA-resolved abstract type.
        #[cfg(feature = "xsd11")]
        let abstract_type_invalid = if has_type_alternatives {
            false
        } else {
            abstract_ct_info.is_some()
        };
        #[cfg(not(feature = "xsd11"))]
        let abstract_type_invalid = abstract_ct_info.is_some();

        // 7. xsi:nil. Structures §3.3.1: E is ·nilled· "with respect to some
        // element declaration D if and only if … 1 E has xsi:nil = true. 2
        // D.{nillable} = true" — a non-nillable declaration nils nothing: its
        // `xsi:nil` is `cvc-elt.3.1` (below), and the content is checked
        // against the type (XSD 1.0: Element Locally Valid (Type) clause 3.1.3
        // and (Complex Type) clause 2 apply "If clause 3.2 of Element Locally
        // Valid (Element) did not apply", and 3.2 needs {nillable} true).
        let is_nil = elem_data.nillable && matches!(xsi_nil, Some("true") | Some("1"));
        // cvc-elt.3.1 (XSD 1.0 §3.3.4): the mere *presence* of xsi:nil on a
        // non-nillable element is invalid, regardless of value. xsi:nil="false"
        // on a non-nillable element still violates the rule because the element
        // declaration's {nillable} property must be true wherever xsi:nil
        // appears.
        let nillable_violation = xsi_nil.is_some() && !elem_data.nillable;
        // cvc-elt.3.2.2 (§3.3.4.3 clause 3.2.3.2; XSD 1.0 §3.3.4 clause
        // 3.2.2): a ·nilled· element's declaration must have no fixed value.
        let nil_fixed_violation = is_nil && elem_data.fixed_value.is_some();

        DeclaredElementBinding {
            type_key,
            type_source,
            xsi_type_invalid,
            abstract_ct_info,
            abstract_type_invalid,
            is_abstract,
            has_deferred_type_error,
            is_nil,
            nillable_violation,
            nil_fixed_violation,
            #[cfg(feature = "xsd11")]
            has_type_alternatives,
        }
    }

    /// Element-start step 9 (cont.): the element-start diagnostics deferred
    /// until after the push, so their codes land on the child element rather
    /// than the parent — cvc-elt.2, cvc-type.2, cvc-elt.3.1, cvc-elt.3.2.2
    /// and §src-resolve.
    fn report_declared_element_start_errors(
        &mut self,
        local_name: NameId,
        elem_key: ElementKey,
        binding: DeclaredElementBinding,
    ) {
        let DeclaredElementBinding {
            abstract_ct_info,
            is_abstract,
            has_deferred_type_error,
            nillable_violation,
            nil_fixed_violation,
            ..
        } = binding;
        if is_abstract {
            let elem_name = self.schema_set.name_table.resolve(local_name);
            self.report_error(
                "cvc-elt.2",
                format!(
                    "Element '{}' is abstract and cannot appear in instances",
                    elem_name
                ),
            );
        }
        if let Some((ct_name, ct_ns)) = abstract_ct_info {
            // Skip here when CTA will re-evaluate the abstract check against the
            // final governing type (see `end_of_attributes_inner`).
            #[cfg(feature = "xsd11")]
            if binding.has_type_alternatives {
                // deferred to end_of_attributes_inner
            } else {
                let type_name =
                    crate::schema::derivation::format_type_name(self.schema_set, ct_name, ct_ns);
                self.report_error(
                    "cvc-type.2",
                    format!(
                        "Type '{}' is abstract and cannot be used to validate an element",
                        type_name
                    ),
                );
            }
            #[cfg(not(feature = "xsd11"))]
            {
                let type_name =
                    crate::schema::derivation::format_type_name(self.schema_set, ct_name, ct_ns);
                self.report_error(
                    "cvc-type.2",
                    format!(
                        "Type '{}' is abstract and cannot be used to validate an element",
                        type_name
                    ),
                );
            }
        }
        if nillable_violation {
            let elem_name = self.schema_set.name_table.resolve(local_name);
            self.report_native_error(
                "cvc-elt.3.1",
                format!(
                    "Element '{}' is not nillable but xsi:nil was specified",
                    elem_name,
                ),
                errors::NativeCause::NilNotAllowed,
            );
        }
        if nil_fixed_violation {
            let elem_name = self.schema_set.name_table.resolve(local_name);
            self.report_native_error(
                "cvc-elt.3.2.2",
                format!(
                    "Element '{}' has xsi:nil=\"true\" but its declaration has a fixed value",
                    elem_name,
                ),
                errors::NativeCause::NilWithFixedValue,
            );
        }
        if has_deferred_type_error {
            let msg = self.schema_set.arenas.elements[elem_key]
                .deferred_type_error
                .as_ref()
                .map(|d| d.message.as_str())
                .unwrap_or("");
            self.report_error("src-resolve", msg.to_string());
        }
    }

    /// Validate an attribute (string-based lookup)
    pub fn validate_attribute(
        &mut self,
        local_name: &str,
        namespace_uri: &str,
        value: &str,
    ) -> SchemaInfo {
        if !self.continue_run() {
            return SchemaInfo::invalid();
        }
        let name_id = self.schema_set.name_table.add(local_name);
        let ns_id = if namespace_uri.is_empty() {
            None
        } else {
            Some(self.schema_set.name_table.add(namespace_uri))
        };
        self.validate_attribute_by_id(name_id, ns_id, value)
    }

    /// Validate an attribute (NameId fast-path)
    pub fn validate_attribute_by_id(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        // The attribute is the subject of whatever is reported meanwhile.
        let outer = self.current_attribute.replace((namespace, local_name));
        let info = self.validate_attribute_event(local_name, namespace, value);
        self.current_attribute = outer;
        info
    }

    /// Record an attribute of the current element **without validating it**,
    /// if no attribute use of the element's governing type matches it: the
    /// attribute is then present but not assessed — as if the type's
    /// attribute wildcard had matched it with `{process contents}` `skip`, of
    /// which Structures §3.10.1 says: "No constraints at all: the item must
    /// simply be well-formed XML". For a host that must not have some
    /// undeclared attributes checked against the schema — for example
    /// `wsu:Id`, which a signature layer adds to signed payload elements whose
    /// schemas do not declare it. The drivers call this when
    /// [`ValidationEventHandler::skip_attribute`](super::ValidationEventHandler::skip_attribute)
    /// asks them to.
    ///
    /// **A declared attribute is never skipped.** A wildcard governs only
    /// the attributes no attribute use matches (§3.4.4.2 Element Locally
    /// Valid (Complex Type), clause 2.1 "If there is among the {attribute
    /// uses} an attribute use U whose {attribute declaration} has the same
    /// expanded name as A …" before 2.2 "otherwise … There is an {attribute
    /// wildcard}"), so an attribute that an attribute use matches is validated
    /// as by [`validate_attribute`](Self::validate_attribute) — its type, its
    /// `fixed` value, its part in ID/IDREF and identity constraints — and the
    /// returned [`SchemaInfo`] is that validation's. For an element with type
    /// alternatives (XSD 1.1) the decision is made against the selected type,
    /// after all attributes: the returned `SchemaInfo` is then marked
    /// `deferred_by_cta`, and the final one comes with the other deferred
    /// results (`take_deferred_attribute_results`).
    ///
    /// What a skipped attribute is:
    ///
    /// - **Its PSVI**: none — the returned [`SchemaInfo`] has no type and no
    ///   declaration, `[validity]` *notKnown* and `[validation attempted]`
    ///   *none* (§3.2.5.1 applies only to an attribute whose validity "has
    ///   been assessed"). The typed document builder leaves it untyped.
    /// - **Its element**: an attribute the type does not allow is no error,
    ///   so it does not make the element invalid; but the element is no
    ///   longer fully assessed — its `[validation attempted]` is at most
    ///   *partial* (§3.3.5.1, "3 otherwise partial").
    /// - **Presence**: it is one of E.\[attributes\]: a second attribute of
    ///   the same name is a duplicate, and type alternatives (§3.12.4) and
    ///   assertions (§3.13.4.1) see it, untyped.
    /// - **ID/IDREF**: it has no type and no `[schema actual value]`, so it is
    ///   not in the ·eligible item set· of the ID/IDREF table (§3.17.5.2).
    /// - **Identity constraints**: like an attribute matched by a skip
    ///   wildcard, a field that selects it gets no value (so an `xs:key`
    ///   reports the field missing), and it counts when the field selects
    ///   more than one node.
    ///
    /// `xml:base` is still applied (base-URI tracking is not validation).
    /// Attributes in the XML Schema instance namespace are best left to
    /// [`validate_attribute`](Self::validate_attribute): `xsi:type` and
    /// `xsi:nil` have already taken effect at the element start, and a
    /// skipped `xsi:schemaLocation` records no hint; the drivers never offer
    /// them to the hook.
    pub fn skip_attribute(
        &mut self,
        local_name: &str,
        namespace_uri: &str,
        value: &str,
    ) -> SchemaInfo {
        if !self.continue_run() {
            return SchemaInfo::invalid();
        }
        let name_id = self.schema_set.name_table.add(local_name);
        let ns_id = if namespace_uri.is_empty() {
            None
        } else {
            Some(self.schema_set.name_table.add(namespace_uri))
        };
        self.skip_attribute_by_id(name_id, ns_id, value)
    }

    /// [`skip_attribute`](Self::skip_attribute) by interned name.
    pub fn skip_attribute_by_id(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        let outer = self.current_attribute.replace((namespace, local_name));
        let info = self.skip_attribute_event(local_name, namespace, value);
        self.current_attribute = outer;
        info
    }

    fn skip_attribute_event(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        if !self.continue_run() {
            return SchemaInfo::invalid();
        }
        if !self.current_state.can_validate_attribute() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "Attribute validation not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return SchemaInfo::invalid();
        }
        let Some(ev_state) = self.validation_stack.last() else {
            self.report_error("cvc-complex-type", "No element context for attribute");
            return SchemaInfo::invalid();
        };
        #[cfg(feature = "xsd11")]
        let deferred = ev_state.has_type_alternatives;
        #[cfg(not(feature = "xsd11"))]
        let deferred = false;
        // Structures §3.4.4.2 clause 2.1 comes before 2.2: an attribute an
        // attribute use matches is validated by it, never by a wildcard. With
        // type alternatives the governing type is known only after the
        // selection; the decision waits until then.
        if !deferred
            && self.attribute_use_matches(
                ev_state.schema_type,
                ev_state.process_contents,
                local_name,
                namespace,
            )
        {
            return self.validate_attribute_event(local_name, namespace, value);
        }
        // Visible, untyped, to the assertions of the element.
        #[cfg(feature = "xsd11")]
        let _ = self.buffer_fragment_attribute(local_name, namespace, value);
        if namespace == Some(well_known::XML_NAMESPACE)
            && local_name == self.schema_set.name_table.add("base")
        {
            self.apply_xml_base_attribute(value);
        }
        let Some(ev_state) = self.validation_stack.last_mut() else {
            return SchemaInfo::invalid();
        };
        if !ev_state.seen_attributes.insert((namespace, local_name)) {
            return self.reject_duplicate_attribute(local_name);
        }
        self.current_state = ValidatorState::Attribute;
        // Collected with the others, so type alternatives see it; validated
        // or skipped after the selection (`validate_deferred_attributes`).
        #[cfg(feature = "xsd11")]
        if deferred {
            self.skipped_collected
                .push(ev_state.collected_attributes.len());
            ev_state
                .collected_attributes
                .push((namespace, local_name, value.to_string()));
            return SchemaInfo {
                deferred_by_cta: true,
                ..SchemaInfo::empty()
            };
        }
        self.record_skipped_attribute(local_name, namespace);
        SchemaInfo::empty()
    }

    /// Whether an attribute use of the governing type `schema_type` matches
    /// the attribute (Structures §3.4.4.2 clause 2.1). A skip-processed
    /// element validates no attribute.
    fn attribute_use_matches(
        &self,
        schema_type: Option<TypeKey>,
        process_contents: ContentProcessing,
        local_name: NameId,
        namespace: Option<NameId>,
    ) -> bool {
        if process_contents == ContentProcessing::Skip {
            return false;
        }
        match schema_type {
            Some(TypeKey::Complex(ct_key)) => matches!(
                self.find_attribute_in_type(
                    &self.schema_set.arenas.complex_types[ct_key],
                    local_name,
                    namespace
                ),
                AttributeLookup::Found(..)
            ),
            _ => false,
        }
    }

    /// The effects of a skipped attribute on its element: `[validation
    /// attempted]` none for the attribute (§3.3.5.1), so the element is at
    /// most partial; and a field that selects it counts as a match with no
    /// value, as for an attribute a skip wildcard matched.
    fn record_skipped_attribute(&mut self, local_name: NameId, namespace: Option<NameId>) {
        if let Some(ev_state) = self.validation_stack.last_mut() {
            ev_state.any_attr_not_full = true;
        }
        self.count_skipped_attribute_ic_matches(local_name, namespace);
    }

    /// [`validate_attribute_by_id`](Self::validate_attribute_by_id) once the
    /// attribute is the current diagnostic subject.
    fn validate_attribute_event(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        if !self.continue_run() {
            return SchemaInfo::invalid();
        }
        // 1. State machine check
        if !self.current_state.can_validate_attribute() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "Attribute validation not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return SchemaInfo::invalid();
        }

        // Forward attribute to assertion fragment builder (XSD 1.1).
        // Done before ev_state borrow to avoid borrow conflict.
        // The returned attr_ref is stashed so we can install the schema
        // binding (type/decl/content_type) once validation has resolved
        // the attribute's type — without a binding, navigator.typed_value()
        // returns Untyped and assertion XPath value-comparisons reject the
        // attribute as xs:untypedAtomic (§3.13.4.1, §3.5).
        #[cfg(feature = "xsd11")]
        let fragment_attr_ref = self.buffer_fragment_attribute(local_name, namespace, value);

        // Detect xml:base unconditionally (regardless of ALLOW_XML_ATTRIBUTES)
        // and update the current element's base URI for schema-location hints.
        // xml:base is compositional: relative values resolve against the
        // inherited base URI (RFC 3986 §5).
        // Only apply if this is the first xml:base on this element (skip
        // duplicates so an invalid repeated xml:base doesn't overwrite).
        if namespace == Some(well_known::XML_NAMESPACE)
            && local_name == self.schema_set.name_table.add("base")
        {
            self.apply_xml_base_attribute(value);
        }

        let ev_state = match self.validation_stack.last_mut() {
            Some(s) => s,
            None => {
                self.report_error("cvc-complex-type", "No element context for attribute");
                return SchemaInfo::invalid();
            }
        };

        // Validate xsi:* built-in attributes with proper type information
        if namespace == Some(well_known::XSI_NAMESPACE) {
            // Among E's [attributes], which type alternatives see (§3.12.4
            // clause 1.1.2).
            #[cfg(feature = "xsd11")]
            if ev_state.has_type_alternatives {
                self.cta_unlisted_attributes
                    .push((namespace, local_name, value.to_string()));
            }
            let result = self.validate_xsi_attribute_on_element(local_name, namespace, value);
            self.fold_attribute_validity(&result);
            #[cfg(feature = "xsd11")]
            self.bind_fragment_attribute(fragment_attr_ref, &result);
            return result;
        }

        // Skip xml:* attributes when ALLOW_XML_ATTRIBUTES is set
        if namespace == Some(well_known::XML_NAMESPACE)
            && self.flags.contains(ValidationFlags::ALLOW_XML_ATTRIBUTES)
        {
            #[cfg(feature = "xsd11")]
            if ev_state.has_type_alternatives {
                self.cta_unlisted_attributes
                    .push((namespace, local_name, value.to_string()));
            }
            self.current_state = ValidatorState::Attribute;
            return SchemaInfo::empty();
        }

        // 2. Duplicate check
        let attr_pair = (namespace, local_name);
        if !ev_state.seen_attributes.insert(attr_pair) {
            return self.reject_duplicate_attribute(local_name);
        }

        // When type alternatives are active, defer type-dependent attribute
        // validation until after CTA selection in validate_end_of_attributes().
        // Only collect the attribute data and perform type-independent checks.
        #[cfg(feature = "xsd11")]
        if ev_state.has_type_alternatives {
            ev_state
                .collected_attributes
                .push((namespace, local_name, value.to_string()));
            self.current_state = ValidatorState::Attribute;
            // Post-process without type info (IC field matching still works;
            // ID/IDREF will be handled during deferred validation).
            return SchemaInfo {
                deferred_by_cta: true,
                ..SchemaInfo::empty()
            };
        }

        // Determine effective type for attribute validation.
        // When schema_type is None (no governing type) and the element is under
        // lax assessment, use xs:anyType which has anyAttribute processContents=lax.
        let type_key = ev_state.schema_type;
        let process_contents = ev_state.process_contents;

        // When the element is skip-processed (matched by a processContents="skip"
        // xs:any wildcard), accept all attributes without schema validation.
        // The element is not assessed, so its attributes cannot generate cvc-complex-type
        // errors regardless of whether the element has a global declaration.
        if process_contents == ContentProcessing::Skip {
            self.current_state = ValidatorState::Attribute;
            let result = SchemaInfo::empty();
            self.post_process_attribute(local_name, namespace, value, &result);
            return result;
        }

        let ct_key = match self.resolve_attribute_owner_type(
            type_key,
            process_contents,
            local_name,
            namespace,
            value,
        ) {
            AttributeOwnerType::Complex(ct_key) => ct_key,
            AttributeOwnerType::NotAllowed(result) => {
                self.fold_attribute_validity(&result);
                return result;
            }
        };

        self.current_state = ValidatorState::Attribute;
        let result = self.validate_attribute_on_element(ct_key, local_name, namespace, value);
        self.fold_attribute_validity(&result);
        #[cfg(feature = "xsd11")]
        self.bind_fragment_attribute(fragment_attr_ref, &result);
        result
    }

    /// An attribute whose `[validity]` is invalid makes its element invalid:
    /// §3.3.5.1 `[validity]` clause 1.1.2, "Neither its \[children\] nor its
    /// \[attributes\] contains an information item … whose \[validity\] is
    /// invalid". Most attribute checks already mark the element where they
    /// report; this also covers those that only return an invalid
    /// `SchemaInfo` (xsi:* values, an attribute on a simple-typed element, the
    /// XSD 1.0 single-ID-attribute rule). A laxly assessed element is
    /// `notKnown` (clause 2) and is left alone.
    fn fold_attribute_validity(&mut self, attribute: &SchemaInfo) {
        if attribute.validity == SchemaValidity::Invalid {
            self.invalidate_attribute_owner();
        }
    }

    /// The current element carries an invalid attribute (see
    /// [`fold_attribute_validity`](Self::fold_attribute_validity)).
    fn invalidate_attribute_owner(&mut self) {
        if let Some(owner) = self.validation_stack.last_mut() {
            if owner.strictly_assessed {
                invalidate(&mut owner.validity);
            }
        }
    }

    /// Forward the attribute to the assertion fragment builder and return the
    /// buffered node's reference, so the caller can install its schema binding
    /// once validation has resolved the attribute's type. `None` when no
    /// fragment buffering is active or the buffer operation was aborted.
    #[cfg(feature = "xsd11")]
    fn buffer_fragment_attribute(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> Option<u32> {
        if self.is_buffering_assertions() {
            let local = self.schema_set.name_table.resolve(local_name);
            let ns = namespace
                .map(|id| self.schema_set.name_table.resolve(id).to_string())
                .unwrap_or_default();
            let attempt = self
                .fragment_builder
                .as_mut()
                .map(|b| b.attribute(&local, &ns, "", value));
            match attempt {
                Some(Ok(r)) => Some(r),
                Some(Err(e)) => {
                    self.abort_assertion_buffer_op("attribute", e);
                    None
                }
                None => None,
            }
        } else {
            None
        }
    }

    /// Apply an `xml:base` attribute to the current element's base URI and
    /// rebase the schema-location hints already recorded for it. Only the first
    /// `xml:base` on an element applies, so an invalid repeated one cannot
    /// overwrite it.
    fn apply_xml_base_attribute(&mut self, value: &str) {
        let mut xml_base_rebase = None;
        if let Some(ev) = self.validation_stack.last_mut() {
            if !ev.base_uri_set_by_xml_base {
                let xml_base = value.trim();
                let base_uri = resolve_base_uri(xml_base, &ev.base_uri);
                ev.base_uri = base_uri.clone();
                ev.base_uri_set_by_xml_base = true;
                xml_base_rebase = Some((
                    base_uri,
                    ev.schema_location_hint_start,
                    ev.no_namespace_schema_location_hint_start,
                ));
            }
        }
        if let Some((base_uri, sl_start, nnsl_start)) = xml_base_rebase {
            self.rebase_hint_range(sl_start, nnsl_start, &base_uri);
        }
    }

    /// Validate an `xsi:*` built-in attribute with proper type information and
    /// fold the result into the owning element: record it in `seen_attributes`
    /// (so a user-declared `use="required"` attribute use targeting `xsi:type` /
    /// `xsi:nil` / `xsi:schemaLocation` / `xsi:noNamespaceSchemaLocation` is
    /// satisfied by the instance attribute), slice off its error codes, and
    /// track attribute `[validation attempted]` on the parent (§3.3.5.1).
    fn validate_xsi_attribute_on_element(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        self.current_state = ValidatorState::Attribute;
        let ec_snapshot = self
            .validation_stack
            .last()
            .map(|ev| ev.error_codes.len())
            .unwrap_or(0);
        let mut result = self.validate_xsi_attribute(local_name, value);
        if let Some(ev) = self.validation_stack.last_mut() {
            // Record xsi:* attributes in seen_attributes so user-declared
            // attribute uses targeting xsi:type / xsi:nil / xsi:schemaLocation /
            // xsi:noNamespaceSchemaLocation (use="required") are satisfied
            // by the matching instance attribute. Without this insert,
            // `missing_required_attributes` would always report a missing
            // required xsi:* attribute (saxon complex009/complex010).
            ev.seen_attributes.insert((namespace, local_name));
            // Extract attribute-specific error codes (mirrors normal attribute path)
            if ev.error_codes.len() > ec_snapshot {
                result.schema_error_codes = ev.error_codes[ec_snapshot..].to_vec();
                ev.error_codes.truncate(ec_snapshot);
            }
            // Track attribute [validation attempted] on parent element (§3.3.5.1)
            match result.validation_attempted {
                ValidationAttempted::Full => {
                    ev.any_attr_not_none = true;
                }
                ValidationAttempted::None => {
                    ev.any_attr_not_full = true;
                }
                ValidationAttempted::Partial => {
                    ev.any_attr_not_full = true;
                    ev.any_attr_not_none = true;
                }
            }
        }
        // XSD IC field XPaths (e.g. @*) match all attributes including xsi:*.
        // Feed xsi: attributes to IC field matching so that a field selecting
        // @* on an element with both schema and xsi: attributes correctly
        // detects the multi-node condition (cvc-identity-constraint.4.2.1).
        self.post_process_attribute(local_name, namespace, value, &result);
        result
    }

    /// Reject a repeated attribute on the current element (cvc-complex-type.3).
    fn reject_duplicate_attribute(&mut self, local_name: NameId) -> SchemaInfo {
        let attr_name = self.schema_set.name_table.resolve(local_name);
        self.report_error(
            "cvc-complex-type.3",
            format!("Duplicate attribute '{}'", attr_name),
        );
        if let Some(s) = self.validation_stack.last_mut() {
            s.validity = SchemaValidity::Invalid;
        }
        self.current_state = ValidatorState::Attribute;
        let mut result = SchemaInfo::invalid();
        result.schema_error_codes = vec!["cvc-complex-type.3"];
        result
    }

    /// Determine the complex type the attribute must be validated against.
    ///
    /// Lax assessment with no governing type falls back to `xs:anyType` (whose
    /// `anyAttribute` has `processContents="lax"`, so it accepts any attribute);
    /// a simple-typed element admits no attribute use at all, which is a
    /// cvc-complex-type.3.2.1 violation reported here.
    fn resolve_attribute_owner_type(
        &mut self,
        type_key: Option<TypeKey>,
        process_contents: ContentProcessing,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> AttributeOwnerType {
        match type_key {
            Some(TypeKey::Complex(ct)) => AttributeOwnerType::Complex(ct),
            None if process_contents != ContentProcessing::Skip => {
                // Lax assessment: validate attributes against xs:anyType
                // (anyAttribute processContents=lax accepts any attribute)
                AttributeOwnerType::Complex(self.schema_set.any_type_key())
            }
            _ => {
                // Simple-typed element: only xsi:* and (when permitted)
                // xml:* attributes are allowed. Any other attribute is a
                // cvc-complex-type.3.2.1 violation (no matching {attribute
                // use} on a simple type). xsi:* is already handled above;
                // xml:* under ALLOW_XML_ATTRIBUTES is also handled above.
                // Anything reaching here is an undeclared attribute on a
                // simple-typed element.
                self.current_state = ValidatorState::Attribute;
                let attr_name = self.schema_set.name_table.resolve(local_name);
                self.report_error(
                    "cvc-complex-type.3.2.1",
                    format!(
                        "Attribute '{}' is not allowed on a simple-typed element",
                        attr_name
                    ),
                );
                let mut result = SchemaInfo::invalid();
                result.schema_error_codes = vec!["cvc-complex-type.3.2.1"];
                self.post_process_attribute(local_name, namespace, value, &result);
                AttributeOwnerType::NotAllowed(result)
            }
        }
    }

    /// Validate the attribute against the governing complex type, then fold the
    /// result into the owning element: slice off the attribute-specific error
    /// codes, track attribute `[validation attempted]` on the parent, and apply
    /// the XSD 1.0 single-`xs:ID`-attribute rule.
    fn validate_attribute_on_element(
        &mut self,
        ct_key: ComplexTypeKey,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        // Snapshot error_codes so we can extract attribute-specific codes
        let ec_snapshot = self
            .validation_stack
            .last()
            .map_or(0, |ev| ev.error_codes.len());
        let mut result = self.validate_attribute_against_type(ct_key, local_name, namespace, value);
        // ct-props-correct.5 (XSD 1.0 only) / cvc-complex-type per-element
        // ID attribute counting: an element instance may carry at most one
        // attribute whose effective type is xs:ID (or derived). XSD 1.1
        // explicitly relaxed this rule (the constraint was removed in the
        // 1.1 errata pass), so the check is gated on `cached_xsd10_id_key`
        // — populated only under XSD 1.0 (None otherwise), avoiding both
        // the per-attribute version check and the per-attribute built-in
        // hash lookup. (attZ014a/b)
        let attr_is_id = self.cached_xsd10_id_key.is_some_and(|id_key| {
            matches!(result.schema_type, Some(TypeKey::Simple(sk))
                if self.schema_set.derives_from(sk, id_key))
        });
        // Slice attribute-specific error codes and remove from parent
        if let Some(ev) = self.validation_stack.last_mut() {
            if ev.error_codes.len() > ec_snapshot {
                result.schema_error_codes = ev.error_codes[ec_snapshot..].to_vec();
                ev.error_codes.truncate(ec_snapshot);
            }
            // Track attribute [validation attempted] on parent element
            match result.validation_attempted {
                ValidationAttempted::Full => {
                    ev.any_attr_not_none = true;
                }
                ValidationAttempted::None => {
                    ev.any_attr_not_full = true;
                }
                ValidationAttempted::Partial => {
                    ev.any_attr_not_full = true;
                    ev.any_attr_not_none = true;
                }
            }
        }
        if attr_is_id {
            let already_seen = self
                .validation_stack
                .last()
                .is_some_and(|ev| ev.seen_id_attr);
            if already_seen {
                let attr_name = self.schema_set.name_table.resolve(local_name).to_string();
                self.report_error(
                    "cvc-complex-type",
                    format!(
                        "Element has more than one attribute of type xs:ID (or a type \
                         derived from xs:ID); '{}' is the second such attribute",
                        attr_name,
                    ),
                );
                result.validity = SchemaValidity::Invalid;
            } else if let Some(ev) = self.validation_stack.last_mut() {
                ev.seen_id_attr = true;
            }
        }
        result
    }

    /// Install a schema binding (type/decl) on the buffered fragment
    /// attribute node so navigator.typed_value() returns the declared
    /// atomic type during XSD 1.1 assertion XPath evaluation. No-op
    /// when no fragment buffering is active or no type was resolved.
    #[cfg(feature = "xsd11")]
    fn bind_fragment_attribute(&mut self, attr_ref: Option<u32>, info: &SchemaInfo) {
        let Some(attr_ref) = attr_ref else { return };
        let Some(type_key) = info.schema_type else {
            return;
        };
        // The assertion's data model is built from the PSVI: a valid
        // attribute's string value is its [schema normalized value] (XDM 1.0
        // §6.3.4).
        if info.validity == SchemaValidity::Valid {
            let schema_set = self.schema_set;
            if let Some(builder) = self.fragment_builder.as_mut() {
                let normalized = super::simple::schema_normalized(
                    builder.attribute_value(attr_ref),
                    type_key,
                    schema_set,
                );
                if let Some(normalized) = normalized {
                    builder.set_attribute_value(attr_ref, &normalized);
                }
            }
        }
        let binding = crate::document::type_remap::NodeSchemaBinding {
            type_key,
            element_decl: None,
            attribute_decl: info.attribute_decl,
            content_type: None,
        };
        let _ = self.install_fragment_binding(attr_ref, binding, "attr binding");
    }

    /// Signal end of attributes; checks for missing required attributes and
    /// returns the (possibly CTA-updated) type info as a [`SchemaInfo`].
    ///
    /// The defaulted attributes of the element (Structures §3.4.4.2, added to
    /// its \[attributes\] by §3.4.5.1) are found in the same walk of the
    /// governing type's {attribute uses} that checks the required ones; take
    /// them with [`take_default_attributes`](Self::take_default_attributes).
    pub fn validate_end_of_attributes(&mut self) -> SchemaInfo {
        self.default_attributes.clear();
        match self.end_of_attributes_inner(true) {
            EndOfAttributesOutcome::Invalid => SchemaInfo::invalid(),
            EndOfAttributesOutcome::Done => SchemaInfo::empty(),
            // CTA selected (and possibly switched) the governing type: rebuild
            // the updated binding info from the stack top — `schema_type`,
            // `content_type`, `validity`, `type_source`, and `cta_selected`
            // were all recorded there by the CTA step.
            #[cfg(feature = "xsd11")]
            EndOfAttributesOutcome::CtaUpdated => {
                let ev = self.validation_stack.last();
                SchemaInfo {
                    schema_type: ev.and_then(|s| s.schema_type),
                    content_type: ev.and_then(|s| s.content_type),
                    validity: ev.map(|s| s.validity).unwrap_or(SchemaValidity::NotKnown),
                    type_source: ev.and_then(|s| s.type_source),
                    cta_selected: ev.map(|s| s.cta_selected).unwrap_or(false),
                    assertion_outcome: None,
                    ..SchemaInfo::empty()
                }
            }
        }
    }

    /// Value-free variant of
    /// [`validate_end_of_attributes`](Self::validate_end_of_attributes) for
    /// throughput drivers that discard the returned `SchemaInfo`. Runs the
    /// identical end-of-attributes step (CTA evaluation, required-attribute
    /// checks, default/fixed processing) without constructing it.
    /// See `PERF_P1_PLAN` §Step 4.
    pub fn validate_end_of_attributes_novalue(&mut self) {
        let _ = self.end_of_attributes_inner(false);
    }

    /// The defaulted attributes of the current element that the last
    /// [`validate_end_of_attributes`](Self::validate_end_of_attributes) found
    /// — the set [`get_default_attributes`](Self::get_default_attributes)
    /// computes, from the walk the end of the attributes makes anyway — in
    /// the order of the governing type's {attribute uses} (its own uses, its
    /// attribute groups', then its base types'). The buffer is emptied; the
    /// next `validate_end_of_attributes` refills it.
    /// [`validate_end_of_attributes_novalue`](Self::validate_end_of_attributes_novalue)
    /// collects none.
    pub fn take_default_attributes(&mut self) -> Vec<DefaultAttribute> {
        std::mem::take(&mut self.default_attributes)
    }

    /// The end-of-attributes step shared by both entry points above:
    /// everything except assembling the returned `SchemaInfo`; with
    /// `collect_defaults`, the defaulted attributes go to
    /// `self.default_attributes`.
    fn end_of_attributes_inner(&mut self, collect_defaults: bool) -> EndOfAttributesOutcome {
        if !self.continue_run() {
            return EndOfAttributesOutcome::Invalid;
        }
        if !self.current_state.can_end_attributes() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "End-of-attributes not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return EndOfAttributesOutcome::Invalid;
        }

        let schema_type = match self.validation_stack.last() {
            Some(s) => s.schema_type,
            None => {
                self.current_state = ValidatorState::EndOfAttributes;
                return EndOfAttributesOutcome::Done;
            }
        };

        // Evaluate type alternatives (XSD 1.1)
        #[cfg(feature = "xsd11")]
        let has_type_alternatives = self
            .validation_stack
            .last()
            .is_some_and(|s| s.has_type_alternatives);

        #[cfg(feature = "xsd11")]
        let (schema_type, cta_switched, cta_selected) = if has_type_alternatives {
            self.evaluate_conditional_type_assignment(schema_type)
        } else {
            (schema_type, false, false)
        };

        // §3.12.4 / §3.3.4.4 (XSD 1.1): the `cvc-type.2` abstract-type check
        // deferred from element start, re-run against the final governing type.
        #[cfg(feature = "xsd11")]
        if has_type_alternatives {
            self.recheck_abstract_type_after_cta(schema_type);
        }

        // When attributes were deferred for CTA, always validate them
        // against the (possibly unchanged) type.
        #[cfg(feature = "xsd11")]
        if has_type_alternatives {
            // Re-detect assertions BEFORE draining collected_attributes,
            // so redetect can replay them into the fragment builder.
            if cta_switched {
                self.redetect_assertions_after_cta(schema_type);
            }
            self.validate_deferred_attributes(schema_type);
        }

        // Record inheritable attributes with default values for propagation (XSD 1.1).
        // Skipped wholesale when the schema declares no inheritable attribute —
        // there is then nothing for `record_inheritable_defaults` to record, and
        // this avoids its per-element attribute-use / attribute-group scan.
        #[cfg(feature = "xsd11")]
        if self.has_inheritable_attributes {
            if let Some(TypeKey::Complex(ct_key)) = schema_type {
                self.record_inheritable_defaults(ct_key);
            }
        }

        // The attribute uses the instance did not carry, in one walk of the
        // governing type's {attribute uses}: the required ones are missing
        // (cvc-complex-type.4); the defaulted ones take part in identity
        // constraints and ID/IDREF.
        if let Some(TypeKey::Complex(ct_key)) = schema_type {
            self.process_absent_attribute_uses(ct_key, collect_defaults);
        }

        // Forward to assertion fragment builder (XSD 1.1)
        #[cfg(feature = "xsd11")]
        if self.is_buffering_assertions() {
            if let Some(builder) = self.fragment_builder.as_mut() {
                builder.end_of_attributes();
            }
        }

        self.current_state = ValidatorState::EndOfAttributes;

        // When CTA switched the type or selected a type, signal it so the
        // value wrapper returns updated binding info (callers such as the
        // typed builder update element bindings from it). The stack top
        // already carries the post-CTA `schema_type`/`content_type`/
        // `validity`/`type_source`/`cta_selected`, so no data needs to flow
        // through the outcome. `cta_switched` implies `cta_selected`; both
        // are spelled out for clarity.
        #[cfg(feature = "xsd11")]
        if cta_switched || cta_selected {
            return EndOfAttributesOutcome::CtaUpdated;
        }
        EndOfAttributesOutcome::Done
    }

    /// End-of-attributes step: conditional type assignment (§3.12.4).
    ///
    /// Evaluates the element declaration's type alternatives against the
    /// element's attributes — the collected ones, the `xsi:` (and accepted
    /// `xml:`) ones, and the CTA-visible inherited ones; when an alternative
    /// selects a different type, the governing type and content model on the
    /// stack top are switched. Returns the (possibly switched) governing type,
    /// whether it changed, and whether an alternative was selected at all.
    #[cfg(feature = "xsd11")]
    fn evaluate_conditional_type_assignment(
        &mut self,
        schema_type: Option<TypeKey>,
    ) -> (Option<TypeKey>, bool, bool) {
        let mut st = schema_type;
        let mut switched = false;
        let mut selected = false;
        let unlisted = std::mem::take(&mut self.cta_unlisted_attributes);
        if let Some(ev_state) = self.validation_stack.last() {
            if let Some(elem_key) = ev_state.element_decl {
                // §3.12.4 clause 1.1.2: "E's [attributes]" — those
                // collected for validation after the selection, and the
                // `xsi:` (and accepted `xml:`) ones, validated already.
                // Clause 1.1.3: include [inherited attributes]
                // that do not have the same expanded name as any of
                // E's [attributes] when building the CTA XDM instance.
                // Use incoming_inherited (the PSVI view) with the
                // CTA-specific same-name exclusion.
                let mut cta_attrs = ev_state.collected_attributes.clone();
                cta_attrs.extend(unlisted);
                let explicit_names: HashSet<(Option<NameId>, NameId)> =
                    cta_attrs.iter().map(|(ns, name, _)| (*ns, *name)).collect();
                for ((ns, name), val) in &ev_state.incoming_inherited {
                    if !explicit_names.contains(&(*ns, *name)) {
                        cta_attrs.push((*ns, *name, val.value.clone()));
                    }
                }
                let (new_type, raised) = self.collect_expression_warnings(|| {
                    super::alternatives::evaluate_type_alternatives(
                        elem_key,
                        ev_state.local_name,
                        ev_state.namespace,
                        &cta_attrs,
                        ev_state.ns_context.as_ref(),
                        Some(self.instance_base_uri.as_str()).filter(|s| !s.is_empty()),
                        self.schema_set,
                    )
                });
                self.report_expression_warnings(raised);
                if let Some(new_type_key) = new_type {
                    selected = true;
                    if Some(new_type_key) != st {
                        let (content_state, content_type) =
                            self.init_content_model(Some(new_type_key));
                        st = Some(new_type_key);
                        switched = true;
                        if let Some(ev) = self.validation_stack.last_mut() {
                            ev.schema_type = Some(new_type_key);
                            ev.content_state = content_state;
                            ev.content_type = Some(content_type);
                        }
                    }
                }
            }
        }
        // Track CTA selection on ev_state regardless of type change.
        // Preserve XsiType as the governing source when xsi:type was
        // applied — per spec xsi:type takes precedence over CTA.
        if selected {
            if let Some(ev) = self.validation_stack.last_mut() {
                ev.cta_selected = true;
                if ev.type_source != Some(TypeSource::XsiType) {
                    ev.type_source = Some(TypeSource::TypeAlternative);
                }
            }
        }
        (st, switched, selected)
    }

    /// End-of-attributes step: §3.12.4 / §3.3.4.4 (XSD 1.1) — when type
    /// alternatives were present, the `cvc-type.2` abstract-type check was
    /// deferred from element start (the declared type may be abstract, e.g.
    /// OpenDRIVE `t_junction`). Now that CTA has selected the *final* governing
    /// type, re-check it. A correctly-authored CTA always selects a concrete
    /// type, so a clean instance validates; only a genuinely abstract governing
    /// type errors.
    #[cfg(feature = "xsd11")]
    fn recheck_abstract_type_after_cta(&mut self, schema_type: Option<TypeKey>) {
        if let Some(TypeKey::Complex(k)) = schema_type {
            if let Some(ct) = self.schema_set.arenas.complex_types.get(k) {
                if ct.is_abstract {
                    let type_name = crate::schema::derivation::format_type_name(
                        self.schema_set,
                        ct.name,
                        ct.target_namespace,
                    );
                    self.report_error(
                        "cvc-type.2",
                        format!(
                            "Type '{}' is abstract and cannot be used to validate an element",
                            type_name
                        ),
                    );
                    self.mark_current_invalid();
                }
            }
        }
    }

    /// End-of-attributes step: the attribute uses of the governing complex
    /// type the instance did not carry, found in one walk of its {attribute
    /// uses} ([`for_each_attribute_use`](Self::for_each_attribute_use)).
    ///
    /// Each required one is missing — cvc-complex-type.4 (§3.4.4.2 clause 4:
    /// "For each attribute use U in {attribute uses}, if U.{required} = true,
    /// then U.{attribute declaration} has the same expanded name as one of the
    /// attribute information items in E.\[attributes\]"), reported in walk
    /// order. Each optional one with a value constraint is a ·defaulted
    /// attribute· (§3.4.4.2), which §3.4.5.1 adds to E.\[attributes\]: its
    /// value takes part in identity constraints (§3.11.4: "default or fixed
    /// value constraints may play a part in ·key sequence·s") and, for an
    /// ID / IDREF / ENTITY type, in the ID/IDREF table (§3.17.5.2) and the
    /// entity check.
    fn process_absent_attribute_uses(&mut self, ct_key: ComplexTypeKey, collect_defaults: bool) {
        let mut missing: Vec<(Option<NameId>, NameId)> = Vec::new();
        let mut ic_defaults: Vec<IcDefault> = Vec::new();
        let mut id_defaults: Vec<(String, XmlValue)> = Vec::new();
        let mut defaults = std::mem::take(&mut self.default_attributes);
        self.collect_absent_attribute_uses(
            ct_key,
            &mut missing,
            &mut ic_defaults,
            &mut id_defaults,
            collect_defaults.then_some(&mut defaults),
        );
        self.default_attributes = defaults;
        if !missing.is_empty() {
            for (ns, name) in missing {
                if !self.continue_run() {
                    return;
                }
                let name_str = self.schema_set.name_table.resolve(name);
                let name = ExpandedName::new(
                    ns.map(|n| self.schema_set.name_table.resolve_ref(n)),
                    &name_str,
                );
                let err = errors::error(
                    "cvc-complex-type.4",
                    format!("Required attribute '{}' is missing", name_str),
                    self.current_location.clone(),
                )
                .with_native_cause(errors::NativeCause::MissingRequiredAttribute { name })
                .with_type_context(TypeKey::Complex(ct_key), self.schema_set);
                self.report_validation_error(err);
            }
            self.mark_current_invalid();
        }
        // Feed IC field matches (borrow-split: active_constraints borrows &mut self)
        let mut multi_node_defaults: Vec<(NameId, usize, usize)> = Vec::new();
        for (name, ns, value, typed_value) in ic_defaults {
            for cs in &mut self.active_constraints {
                let matches = cs.matching_fields(name, ns);
                for field_idx in matches {
                    let already_matched =
                        cs.set_field_value(field_idx, value.clone(), typed_value.clone());
                    if already_matched {
                        multi_node_defaults.push((
                            cs.key_table.table().constraint_name,
                            field_idx,
                            cs.owner_depth,
                        ));
                    }
                }
            }
        }
        for (constraint_name, field_idx, owner_depth) in multi_node_defaults {
            let cname = self
                .schema_set
                .name_table
                .resolve(constraint_name)
                .to_string();
            self.report_error(
                "cvc-identity-constraint.4.2.1",
                format!(
                    "Identity constraint '{}': field {} matches more than one node",
                    cname,
                    field_idx + 1
                ),
            );
            self.invalidate_open_ic_owner(owner_depth);
        }
        // Validate and collect ID/IDREF values from absent defaults.
        // Owner is the current element (attributes bind to their element).
        let default_owner = self
            .validation_stack
            .last()
            .map(|e| e.element_serial)
            .unwrap_or(0);
        for (value, typed_value) in id_defaults {
            self.collect_id_idref(&typed_value, &value, default_owner);
            if self.check_entity_declared(&typed_value) {
                self.invalidate_attribute_owner();
            }
        }
    }

    /// The typed value of a value constraint's lexical form `value` under
    /// `type_key`, a QName / NOTATION read with the bindings in scope at
    /// `source` (the element that carries the constraint; Datatypes §3.3.18:
    /// the mapping "depends on the namespace bindings in scope where the
    /// literal occurs"). `None` when the value is not valid for the type (the
    /// schema is then in error) — the caller compares the lexical form.
    ///
    /// The literal is validated with those bindings, so that a union decides
    /// its member as it would where the literal was written: a QName member
    /// whose facets need the expanded name (an `enumeration` of QNames) is
    /// not passed over for a later `xs:ID` or `xs:string` member.
    fn value_constraint_typed_value(
        &self,
        value: &str,
        type_key: TypeKey,
        source: Option<&SourceRef>,
    ) -> Option<XmlValue> {
        let ctx = self.schema_set.value_constraint_namespaces(source);
        let names = &self.schema_set.name_table;
        let resolver = ctx.map(|ns| super::simple::snapshot_prefixes(ns, names));
        let typed = super::simple::validate_simple_type_in(
            value,
            type_key,
            self.schema_set,
            resolver
                .as_ref()
                .map(|r| r as super::simple::PrefixResolver<'_>),
        )
        .ok()?
        .typed_value;
        match self.resolve_qname_typed_value_in(Some(&typed), value, ctx) {
            Ok(Some(resolved)) => Some(resolved),
            Ok(None) if !super::simple::holds_lexical_qname(&typed) => Some(typed),
            // A QName whose prefix is bound nowhere at the declaration: no
            // value to compare by (the lexical form is used).
            _ => None,
        }
    }

    /// Whether a defaulted attribute of type `type_key` is handed out with its
    /// typed value ([`DefaultAttribute::typed_value`]): when the run retains
    /// PSVI typed values, and always for a type whose values can be (or hold)
    /// `xs:QName` / `xs:NOTATION` values — which the lexical form alone does
    /// not give, since it was written with the schema document's bindings.
    fn default_typed_value_wanted(&self, type_key: TypeKey) -> bool {
        self.flags
            .contains(ValidationFlags::BUILD_PSVI_TYPED_VALUES)
            || super::simple::type_has_qname_values(type_key, self.schema_set)
    }

    /// The `&self` scan behind
    /// [`process_absent_attribute_uses`](Self::process_absent_attribute_uses),
    /// borrowing the element's seen attributes straight from the validation
    /// stack (no per-element clone): the missing required uses, the defaulted
    /// values an active identity constraint may select, and the
    /// ID/IDREF/ENTITY-typed defaulted values that still need validating —
    /// and, when `defaults` is given, every defaulted attribute.
    ///
    /// A defaulted value goes to the identity constraints as its typed value
    /// under the use's type, as an attribute the instance carries does
    /// (`post_process_attribute`): key sequences are compared "pairwise
    /// equal or identical, as defined by Equality and Identity in [XML
    /// Schema: Datatypes]" (§3.11.4 clause 4.1), where "values from different
    /// primitive data spaces are made artificially unequal" (Datatypes
    /// §2.2.2). A QName / NOTATION value is read with the bindings in scope
    /// at the element that carries the value constraint.
    fn collect_absent_attribute_uses(
        &self,
        ct_key: ComplexTypeKey,
        missing: &mut Vec<(Option<NameId>, NameId)>,
        ic_defaults: &mut Vec<IcDefault>,
        id_defaults: &mut Vec<(String, XmlValue)>,
        mut defaults: Option<&mut Vec<DefaultAttribute>>,
    ) {
        let empty_seen = AttrNameSet::default();
        let seen = self
            .validation_stack
            .last()
            .map(|s| &s.seen_attributes)
            .unwrap_or(&empty_seen);
        let has_ic = !self.active_constraints.is_empty();
        self.for_each_attribute_use(ct_key, |u| {
            if seen.contains(&(u.namespace, u.name)) {
                return;
            }
            if u.use_kind == AttributeUseKind::Required {
                missing.push((u.namespace, u.name));
                return;
            }
            let Some(v) = u.value else { return };
            // The value constraint's {value} — computed once, and only when
            // a consumer wants it: the defaulted attribute handed out (PSVI
            // typed values retained, or a QName-capable type), an active
            // identity constraint, the ID/IDREF table and ENTITY check (a type
            // that can hold such values). The last consumer takes it; the
            // others clone it.
            let want_default = defaults.is_some()
                && u.type_key
                    .is_some_and(|tk| self.default_typed_value_wanted(tk));
            let want_id = u
                .type_key
                .is_some_and(|tk| super::simple::type_has_id_family_values(tk, self.schema_set));
            let mut consumers = u8::from(want_default) + u8::from(has_ic) + u8::from(want_id);
            let mut typed = if consumers > 0 {
                u.type_key
                    .and_then(|tk| self.value_constraint_typed_value(v, tk, u.value_source))
            } else {
                None
            };
            let mut next_typed = || {
                consumers -= 1;
                if consumers == 0 {
                    typed.take()
                } else {
                    typed.clone()
                }
            };
            // A default whose type can hold ID / IDREF / ENTITY values — the
            // built-in types, a restriction of one, a list of one, a union
            // with one as a member — joins the ID/IDREF table and the entity
            // check as the typed value of its value constraint (read with the
            // declaration's bindings, so a union picks the member the schema
            // means), exactly as a carried attribute's typed value does.
            if want_id {
                if let Some(typed_value) = next_typed() {
                    id_defaults.push((v.to_string(), typed_value));
                }
            }
            if has_ic {
                ic_defaults.push((
                    u.name,
                    u.namespace.unwrap_or(NameId(0)),
                    v.to_string(),
                    next_typed(),
                ));
            }
            if let Some(defaults) = defaults.as_deref_mut() {
                defaults.push(DefaultAttribute {
                    local_name: u.name,
                    namespace: u.namespace,
                    attribute_key: u.attr_key,
                    type_key: u.type_key,
                    value: v.to_string(),
                    typed_value: if want_default { next_typed() } else { None },
                });
            }
        });
    }

    /// Validate a text content event
    pub fn validate_text(&mut self, text: &str) {
        if !self.continue_run() {
            return;
        }
        if !self.current_state.can_validate_text() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "Text content not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return;
        }

        // Collect errors first to avoid borrow conflicts
        let mut pending_errors: Vec<(&'static str, String, Option<errors::NativeCause>)> =
            Vec::new();
        let has_characters = !text.is_empty();
        let has_non_ws = text
            .bytes()
            .any(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'));

        if let Some(ev_state) = self.validation_stack.last_mut() {
            // 1. Check content type
            if has_characters {
                match ev_state.content_type {
                    Some(ContentType::Empty) => {
                        let elem_name = self
                            .schema_set
                            .name_table
                            .resolve(ev_state.local_name)
                            .to_string();
                        pending_errors.push((
                            "cvc-complex-type.2.1",
                            format!(
                                "Element '{}' has empty content type but text was found",
                                elem_name,
                            ),
                            None,
                        ));
                    }
                    Some(ContentType::ElementOnly) if has_non_ws => {
                        let elem_name = self
                            .schema_set
                            .name_table
                            .resolve(ev_state.local_name)
                            .to_string();
                        pending_errors.push((
                            "cvc-complex-type.2.3",
                            format!(
                                "Element '{}' has element-only content but non-whitespace text was found",
                                elem_name,
                            ),
                    None,
                        ));
                    }
                    _ => {}
                }
            }

            // cvc-elt.3.2.1: nilled elements have no character children,
            // including XML S, regardless of the declared content variety.
            if ev_state.is_nil && has_characters {
                let elem_name = self
                    .schema_set
                    .name_table
                    .resolve(ev_state.local_name)
                    .to_string();
                pending_errors.push((
                    "cvc-elt.3.2.1",
                    format!(
                        "Element '{}' is nilled but has non-empty content",
                        elem_name,
                    ),
                    Some(errors::NativeCause::NilWithContent),
                ));
            }

            // 3. Accumulate text
            ev_state.text_content.push_str(text);
            ev_state.has_text = true;
        }

        // Report collected errors. Each one is a failure of the current
        // element's own local validity — §3.4.4.2 clause 1.1 (empty content
        // type: "E has no character or element information item [children]"),
        // clause 1.3 (element-only: no character children "other than those
        // whose [character code] is defined as a white space"), §3.3.4.3
        // clause 3.2.3.1 (nilled: "E has no character or element information
        // item [children]").
        if !pending_errors.is_empty() {
            self.invalidate_current();
        }
        for (constraint, message, cause) in pending_errors {
            if !self.continue_run() {
                return;
            }
            match cause {
                Some(cause) => self.report_native_error(constraint, message, cause),
                None => self.report_error(constraint, message),
            }
        }

        // Forward text to assertion fragment builder (XSD 1.1)
        #[cfg(feature = "xsd11")]
        if self.is_buffering_assertions() {
            if let Some(builder) = self.fragment_builder.as_mut() {
                builder.text(text);
            }
        }

        self.current_state = ValidatorState::Text;
    }

    /// Validate a whitespace-only text event
    ///
    /// Whitespace is always allowed in element-only content (it is insignificant).
    pub fn validate_whitespace(&mut self, text: &str) {
        if !self.continue_run() {
            return;
        }
        if !self.current_state.can_validate_text() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "Whitespace not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return;
        }

        let mut nil_violation: Option<String> = None;
        let mut empty_violation: Option<String> = None;
        if let Some(ev_state) = self.validation_stack.last_mut() {
            // cvc-complex-type.2.1: when {content type} is empty the element
            // information item must have *no* character information items,
            // including whitespace. Element-only content still treats
            // whitespace as insignificant.
            if !text.is_empty() && matches!(ev_state.content_type, Some(ContentType::Empty)) {
                let elem_name = self
                    .schema_set
                    .name_table
                    .resolve(ev_state.local_name)
                    .to_string();
                empty_violation = Some(format!(
                    "Element '{}' has empty content type but text was found",
                    elem_name,
                ));
            }

            // cvc-elt.3.2.1: XML S still consists of character children;
            // nilled elements cannot contain it, even in element-only content.
            if ev_state.is_nil && !text.is_empty() {
                let elem_name = self
                    .schema_set
                    .name_table
                    .resolve(ev_state.local_name)
                    .to_string();
                nil_violation = Some(format!(
                    "Element '{}' is nilled but has non-empty content",
                    elem_name,
                ));
            }

            // Accumulate (may be needed for TextOnly simple type validation)
            if matches!(
                ev_state.content_type,
                Some(ContentType::TextOnly) | Some(ContentType::Mixed)
            ) {
                ev_state.text_content.push_str(text);
                ev_state.has_text = true;
            } else if !text.is_empty() {
                // White space is character [children] too: an element that has
                // them takes no value constraint (§3.3.4.3 clause 5.1).
                ev_state.has_text = true;
            }
        }

        // Local-validity failures of the current element (as in `validate_text`).
        if empty_violation.is_some() || nil_violation.is_some() {
            self.invalidate_current();
        }
        if let Some(msg) = empty_violation {
            self.report_error("cvc-complex-type.2.1", msg);
        }
        if let Some(msg) = nil_violation {
            self.report_native_error("cvc-elt.3.2.1", msg, errors::NativeCause::NilWithContent);
        }

        // Forward whitespace to assertion fragment builder (XSD 1.1)
        #[cfg(feature = "xsd11")]
        if self.is_buffering_assertions() {
            if let Some(builder) = self.fragment_builder.as_mut() {
                builder.text(text);
            }
        }

        self.current_state = ValidatorState::Whitespace;
    }

    /// Shared prologue for both end-element paths: validate the call-sequence
    /// state, pop the top element, and run the five completion steps (content
    /// model, simple content, assertions, identity constraints, ID bindings) —
    /// all of which mutate stack/runtime state and emit errors as side effects.
    ///
    /// Returns the finished top element (plus, under XSD 1.1, the assertion
    /// outcome the value path folds into the `SchemaInfo`), or `None` if the
    /// call was out of sequence / the stack was empty (the caller then returns
    /// its own invalid sentinel, matching the previous inline behaviour).
    fn finish_top_element(&mut self) -> Option<FinishedElement> {
        if !self.continue_run() {
            return None;
        }
        if !self.current_state.can_end_element() {
            self.report_error(
                "cvc-complex-type",
                format!(
                    "End element not allowed in current state {:?}",
                    self.current_state
                ),
            );
            return None;
        }

        let mut ev_state = match self.validation_stack.pop() {
            Some(s) => s,
            None => {
                self.report_error(
                    "cvc-complex-type",
                    "End element called but validation stack is empty",
                );
                return None;
            }
        };

        // 1. Content model completion (NFA accept-state check, expected-element errors)
        self.finish_content_model(&mut ev_state);
        if !self.continue_run() {
            return None;
        }

        // 2. Simple-content / text validation (typed value, fixed/default)
        self.finish_simple_content(&mut ev_state);
        if !self.continue_run() {
            return None;
        }

        // 2c. Assertion evaluation (XSD 1.1)
        #[cfg(feature = "xsd11")]
        let assertion_outcome = self.finish_assertions(&mut ev_state);

        // 3. Identity-constraint scope teardown (field values, scope exit, keyref retry)
        self.finish_identity_constraints(&mut ev_state);
        if !self.continue_run() {
            return None;
        }

        // 4. ID/IDREF/ENTITY bookkeeping
        self.finish_id_bindings(&mut ev_state);

        // 4b. Validation root closing: its ID/IDREF verdict
        if self.validation_stack.is_empty() {
            self.finish_validation_root_ids(&mut ev_state);
        }

        #[cfg(feature = "xsd11")]
        {
            Some((ev_state, assertion_outcome))
        }
        #[cfg(not(feature = "xsd11"))]
        {
            Some(ev_state)
        }
    }

    /// Validate an element end event, returning the element's [`SchemaInfo`].
    pub fn validate_end_element(&mut self) -> SchemaInfo {
        let finished = match self.finish_top_element() {
            Some(f) => f,
            None => return SchemaInfo::invalid(),
        };
        #[cfg(feature = "xsd11")]
        let (mut ev_state, assertion_outcome) = finished;
        #[cfg(not(feature = "xsd11"))]
        let mut ev_state = finished;

        // 5. PSVI assembly (element-path pop, validity, validation-attempted, SchemaInfo)
        let info = self.assemble_schema_info(
            &mut ev_state,
            #[cfg(feature = "xsd11")]
            assertion_outcome,
        );
        // Recycle the shell — its collection capacity (`text_content`,
        // `seen_attributes`, the inherited maps) is reused by the next element.
        self.state_pool.push(ev_state);
        info
    }

    /// Value-free variant of [`validate_end_element`](Self::validate_end_element)
    /// for throughput drivers that only need the element's validity and discard
    /// the `SchemaInfo`. Runs the identical completion + PSVI side-effect steps
    /// but skips constructing (and dropping) the per-element `SchemaInfo`. The
    /// typed-builder / push-API callers keep the value-returning method.
    /// See `PERF_P1_PLAN` §Step 4.
    pub fn validate_end_element_novalue(&mut self) -> SchemaValidity {
        self.validate_end_element_verdict().0
    }

    /// [`validate_end_element_novalue`](Self::validate_end_element_novalue)
    /// with the element's `[validation attempted]` beside its `[validity]` —
    /// what the streaming driver hands its handler
    /// ([`EndElementInfo`](super::EndElementInfo)).
    pub(crate) fn validate_end_element_verdict(&mut self) -> (SchemaValidity, ValidationAttempted) {
        let finished = match self.finish_top_element() {
            Some(f) => f,
            None => return (SchemaValidity::Invalid, ValidationAttempted::None),
        };
        #[cfg(feature = "xsd11")]
        let (ev_state, _assertion_outcome) = finished;
        #[cfg(not(feature = "xsd11"))]
        let ev_state = finished;

        // Same PSVI side effects (path pop, state machine, [validation attempted]
        // propagation) as the value path — only the `SchemaInfo` literal is skipped.
        let validation_attempted = self.finalize_element(&ev_state);
        let validity = assessed_validity(&ev_state);
        self.state_pool.push(ev_state);
        (validity, validation_attempted)
    }

    /// F2/B1 step 1: content-model completion check (cvc-complex-type.2.4).
    ///
    /// For element-only or mixed content, verify the content model reached an
    /// accept state; if not, report the "incomplete content" error.
    fn finish_content_model(&mut self, ev_state: &mut ElementValidationState) {
        // 1. Check content model completion
        if !ev_state.is_nil {
            let element_content = matches!(
                ev_state.content_type,
                Some(ContentType::ElementOnly) | Some(ContentType::Mixed)
            );
            let complete = if element_content {
                match ev_state.content_state.try_is_complete() {
                    Ok(complete) => complete,
                    Err(limit) => {
                        // Operational failure, not a verdict (§8.4).
                        let elem_name = self
                            .schema_set
                            .name_table
                            .resolve(ev_state.local_name)
                            .to_string();
                        self.fail_operational(
                            "validation-resource-limit",
                            format!(
                                "Completion check of element '{elem_name}' exceeded an \
                                 execution limit — {limit}; assessment aborted"
                            ),
                        );
                        return;
                    }
                }
            } else {
                true
            };
            match ev_state.content_type {
                Some(ContentType::ElementOnly) | Some(ContentType::Mixed) if !complete => {
                    let elem_name = self.schema_set.name_table.resolve(ev_state.local_name);
                    let err = errors::error(
                        "cvc-complex-type.2.4",
                        format!(
                            "Element '{}' content model is incomplete: expected more child elements",
                            elem_name,
                        ),
                        self.current_location.clone(),
                    );
                    let mut err = err.with_native_cause(errors::NativeCause::IncompleteContent);
                    if let Some(key) = ev_state.schema_type {
                        err = err.with_type_context(key, self.schema_set);
                    }
                    self.report_validation_error_to(err, &mut ev_state.error_codes);
                    ev_state.validity = SchemaValidity::Invalid;
                }
                _ => {}
            }
        }
    }

    /// PERF_LAZY_PSVI Phase 2: when PSVI value retention is opted out
    /// (`BUILD_PSVI_TYPED_VALUES` clear) and no consumer needs the typed value,
    /// validate the element's simple content without materializing an
    /// `XmlValue`. Returns `true` if it handled validation (the caller then
    /// skips the value-building path); `false` to fall back.
    ///
    /// Gated off when an identity constraint is active (its fields may read the
    /// value) or the declaration has a fixed/default (compared in value space);
    /// [`super::simple::try_validate_lexical_only`] further restricts
    /// eligibility to accept-any-input string-family types.
    /// PERF_LAZY_PSVI Phase 2 gate shared by the element and attribute fast
    /// paths: PSVI value retention is opted out (`BUILD_PSVI_TYPED_VALUES`
    /// clear) *and* no identity constraint is active that might read this
    /// node's value. Callers additionally exclude nodes with a fixed/default
    /// value, which need a value-space comparison.
    fn psvi_retention_disabled(&self) -> bool {
        !self
            .flags
            .contains(ValidationFlags::BUILD_PSVI_TYPED_VALUES)
            && self.active_constraints.is_empty()
    }

    fn try_fast_simple_content(
        &mut self,
        ev_state: &mut ElementValidationState,
        schema_type: TypeKey,
    ) -> bool {
        if !self.psvi_retention_disabled() {
            return false;
        }
        let has_fixed_or_default = ev_state.element_decl.is_some_and(|k| {
            let d = &self.schema_set.arenas.elements[k];
            d.default_value.is_some() || d.fixed_value.is_some()
        });
        if has_fixed_or_default {
            return false;
        }
        match super::simple::try_validate_lexical_only(
            &ev_state.text_content,
            schema_type,
            self.schema_set,
        ) {
            Some(Ok(())) => true,
            Some(Err(err)) => {
                self.report_validation_error_to(err, &mut ev_state.error_codes);
                ev_state.validity = SchemaValidity::Invalid;
                true
            }
            None => false,
        }
    }

    /// Clause 5.1.1 of Element Locally Valid (Element) (§3.3.4.3) for an
    /// element whose governing type an `xsi:type` gave and whose content is
    /// empty or element-only: "If E's ·governing type definition· is an
    /// ·instance-specified type definition·, then D.{value constraint} is a
    /// valid default for the ·governing type definition· as defined in
    /// Element Default Valid (Immediate) (§3.3.6.2)", whose clause 2 requires
    /// "2.1 T.{content type}.{variety} = mixed. 2.2 The particle T.{content
    /// type}.{particle} is ·emptiable·". Clause 5.1 applies only to an element
    /// with "neither element nor character [children]" that is not ·nilled·.
    /// (Mixed content without an emptiable particle fails the content model
    /// of the element, which has no children, `cvc-complex-type.2.4`.)
    fn check_default_for_complex_override(&mut self, ev_state: &mut ElementValidationState) {
        if ev_state.is_nil
            || ev_state.has_text
            || ev_state.has_element_children
            || ev_state.type_source != Some(TypeSource::XsiType)
            || !matches!(
                ev_state.content_type,
                Some(ContentType::Empty) | Some(ContentType::ElementOnly)
            )
        {
            return;
        }
        let Some(elem_key) = ev_state.element_decl else {
            return;
        };
        let decl = &self.schema_set.arenas.elements[elem_key];
        let (kind, value) = match (&decl.default_value, &decl.fixed_value) {
            (Some(value), _) => ("default", value),
            (None, Some(value)) => ("fixed", value),
            (None, None) => return,
        };
        let elem_name = self.schema_set.name_table.resolve(ev_state.local_name);
        let err = errors::error(
            "cvc-elt.5.1.1",
            format!(
                "Element '{elem_name}' is empty and takes its {kind} value '{value}', which is \
                 not a valid default for its xsi:type: a type whose content is neither simple \
                 nor mixed"
            ),
            self.current_location.clone(),
        );
        self.report_validation_error_to(err, &mut ev_state.error_codes);
        ev_state.validity = SchemaValidity::Invalid;
    }

    /// Clause 5.1.1 for a simple governing type an `xsi:type` gave: Simple
    /// Default Valid (§3.2.6.2) "2 V.{lexical form} maps to V.{value} in the
    /// value space of T" — the element's value (its typed value, read from
    /// the {lexical form} under the governing type) is equal or identical to
    /// the declaration's {value}. Not compared when the declared type has no
    /// value space of its own (`xs:anySimpleType`: an untyped {value}), or
    /// when the {value} is not known (the schema is then in error).
    fn check_default_maps_to_value(&mut self, ev_state: &mut ElementValidationState) {
        let (Some(elem_key), Some(value)) = (ev_state.element_decl, ev_state.typed_value.as_ref())
        else {
            return;
        };
        let Some(record) = self.schema_set.element_value_constraint(elem_key) else {
            return;
        };
        let Some(declared) = record.typed_value.as_ref() else {
            return;
        };
        if declared.type_code == XmlTypeCode::UntypedAtomic
            || super::simple::typed_values_equal(declared, value)
        {
            return;
        }
        let elem_name = self.schema_set.name_table.resolve(ev_state.local_name);
        let err = errors::error(
            "cvc-elt.5.1.1",
            format!(
                "Element '{elem_name}' is empty and takes the value '{}', which its xsi:type \
                 reads as another value than the declaration's",
                record.lexical_form
            ),
            self.current_location.clone(),
        );
        self.report_validation_error_to(err, &mut ev_state.error_codes);
        ev_state.validity = SchemaValidity::Invalid;
    }

    /// F2/B1 step 2: simple-content / text validation.
    ///
    /// For text-only content: substitute default/fixed for an empty element,
    /// run simple-type validation, perform the QName/NOTATION prefix-binding
    /// check, and verify any fixed value (cvc-elt.5.2.2). Also enforces the
    /// mixed-content fixed-value rules (cvc-elt.5.2.2.1 / 5.2.2.2.1).
    fn finish_simple_content(&mut self, ev_state: &mut ElementValidationState) {
        // Clause 5.1.1 for a governing type without simple content.
        self.check_default_for_complex_override(ev_state);
        // 2. For TextOnly: validate text content against simple type
        if ev_state.content_type == Some(ContentType::TextOnly) && !ev_state.is_nil {
            // Clause 5.1: an empty element takes its value constraint; the
            // fixed-value comparison of clause 5.2.2 is then not made.
            let mut substituted_fixed = false;
            // Whether another type than the declared one governs the element
            // that takes its value constraint (`xsi:type`, a type alternative).
            let mut overridden = false;
            // Handle default value before validation: if the element has no text
            // content and has a default, substitute the default value so that
            // simple-type validation runs against it (not the empty string).
            if let Some(elem_key) = ev_state.element_decl {
                let schema_set: &'a SchemaSet = self.schema_set;
                let elem_data = &schema_set.arenas.elements[elem_key];
                if !ev_state.has_text && !ev_state.has_element_children {
                    if let Some(default_value) = &elem_data.default_value {
                        ev_state.is_default = true;
                        ev_state.text_content = default_value.clone();
                    } else if let Some(fixed_value) = &elem_data.fixed_value {
                        // XSD spec §3.3.4.3: "If fixed is specified, then the
                        // element's content must either be empty, in which case
                        // fixed behaves as default, or match fixed." Clause 5.1
                        // applied: [schema specified] is "schema" (§3.3.5.5).
                        ev_state.is_default = true;
                        ev_state.text_content = fixed_value.clone();
                        substituted_fixed = true;
                    }
                    // Clause 5.1.2: "The element information item with D.{value
                    // constraint}.{lexical form} used as its ·normalized value·
                    // is locally ·valid· with respect to the ·governing type
                    // definition·". Under the declared type the attribute as
                    // written normalizes to that form; under another type the
                    // form itself is read (its white space may differ: a union
                    // member's, `xs:anySimpleType`'s).
                    let declared = elem_data
                        .resolved_type
                        .unwrap_or_else(|| TypeKey::Complex(schema_set.any_type_key()));
                    if ev_state.is_default && ev_state.schema_type != Some(declared) {
                        if let Some(lexical) = schema_set.value_constraint_lexical_form(elem_data) {
                            ev_state.text_content = lexical;
                            overridden = true;
                        }
                    }
                }
            }

            if let Some(schema_type) = ev_state.schema_type {
                if !self.try_fast_simple_content(ev_state, schema_type) {
                    // QName / NOTATION values resolve their prefixes with the
                    // element's in-scope namespaces (Datatypes §3.3.18) — the
                    // bindings where the literal occurs. A value constraint
                    // the element took (clause 5.1) was written on its
                    // declaration: its literal is read with the schema
                    // document's bindings there, and its value is the
                    // constraint's {value} (Structures §3.3.2: "the ·actual
                    // value· … of the [attribute]"; §3.3.5.4 Note: without
                    // ·namespace fixup· "the prefix used in the lexical
                    // representation … will not necessarily map to the
                    // namespace name of the value").
                    let schema_set: &'a SchemaSet = self.schema_set;
                    let declaration_ctx = ev_state
                        .element_decl
                        .filter(|_| ev_state.is_default)
                        .and_then(|k| schema_set.arenas.elements.get(k))
                        .and_then(|d| schema_set.value_constraint_namespaces(d.source.as_ref()));
                    let qname_ctx = if ev_state.is_default {
                        declaration_ctx
                    } else {
                        ev_state.ns_context.as_ref()
                    };
                    let names = &self.schema_set.name_table;
                    let resolver = qname_ctx.map(|ns| super::simple::snapshot_prefixes(ns, names));
                    let (outcome, raised) = self.collect_expression_warnings(|| {
                        super::simple::validate_simple_type_in(
                            &ev_state.text_content,
                            schema_type,
                            self.schema_set,
                            resolver
                                .as_ref()
                                .map(|r| r as super::simple::PrefixResolver<'_>),
                        )
                    });
                    match outcome {
                        Ok(result) => {
                            self.note_decimal_capacity(&result.typed_value);
                            self.note_seconds_capacity(&result.typed_value, &ev_state.text_content);
                            self.report_expression_warnings(raised);
                            ev_state.member_type = result.member_type;
                            ev_state.typed_value = Some(result.typed_value);
                            // §3.3.5.4 [schema normalized value] clause 1.1:
                            // "If clause 5.1 … has applied, then the {lexical
                            // form} of the {value constraint}" — the form read
                            // above under another type, else the attribute
                            // normalized for the declared type (as written for
                            // `xs:anySimpleType`, which normalizes nothing).
                            ev_state.normalized_value = if overridden {
                                Some(ev_state.text_content.clone())
                            } else if ev_state.is_default {
                                result
                                    .normalized_value
                                    .or_else(|| Some(ev_state.text_content.clone()))
                            } else {
                                result.normalized_value
                            };

                            // QName/NOTATION prefix-binding check (Datatypes
                            // §3.3.18 / §3.3.19): the simple-type validator
                            // doesn't see namespaces, so an undeclared prefix
                            // (e.g. `xmlns:xsi` per W3C bug 4053) must be
                            // flagged here against the element's snapshot.
                            // This applies to a single (atomic) QName/NOTATION
                            // value; list members are checked one by one below.
                            // A union value holding a QName or NOTATION
                            // value is checked as the atomic value it holds.
                            let needs_qname_check =
                                ev_state.typed_value.as_ref().is_some_and(|v| {
                                    !super::simple::holds_list(v)
                                        && (matches!(
                                            v.value,
                                            crate::types::value::XmlValueKind::Atomic(_)
                                        ) || super::simple::holds_lexical_qname(v))
                                        && (v.type_code == XmlTypeCode::QName
                                            || v.type_code == XmlTypeCode::Notation)
                                });
                            if needs_qname_check && ev_state.validity == SchemaValidity::Valid {
                                if let Some(ctx) = qname_ctx {
                                    let raw = ev_state.text_content.trim();
                                    if let Some(colon_pos) = raw.find(':') {
                                        let prefix = &raw[..colon_pos];
                                        let prefix_id = self.schema_set.name_table.add(prefix);
                                        if ctx.resolve_prefix(prefix_id).is_none() {
                                            let elem_name = self
                                                .schema_set
                                                .name_table
                                                .resolve(ev_state.local_name)
                                                .to_string();
                                            let err = errors::error(
                                            "cvc-datatype-valid.1.2.1",
                                            format!(
                                                "Element '{}' has QName value '{}' with undeclared prefix '{}'",
                                                elem_name, raw, prefix
                                            ),
                                            self.current_location.clone(),
                                        );
                                            self.report_validation_error_to(
                                                err,
                                                &mut ev_state.error_codes,
                                            );
                                            ev_state.validity = SchemaValidity::Invalid;
                                        }
                                    }
                                }
                            }
                            // The same check for every QName / NOTATION member
                            // of a list value (a list of `xs:QName`, or of a
                            // union with a QName or NOTATION member type).
                            let list_value = ev_state
                                .typed_value
                                .as_ref()
                                .filter(|v| super::simple::holds_list(v));
                            if let (Some(value), Some(ctx), SchemaValidity::Valid) =
                                (list_value, qname_ctx, ev_state.validity)
                            {
                                match resolve_list_qname_members(
                                    value,
                                    ctx,
                                    &self.schema_set.name_table,
                                ) {
                                    Ok(Some(resolved)) => ev_state.typed_value = Some(resolved),
                                    Ok(None) => {}
                                    Err(prefix) => {
                                        let elem_name = self
                                            .schema_set
                                            .name_table
                                            .resolve(ev_state.local_name)
                                            .to_string();
                                        let err = errors::error(
                                            "cvc-datatype-valid.1.2.1",
                                            format!(
                                                "Element '{}' has QName value '{}' with undeclared prefix '{}'",
                                                elem_name,
                                                ev_state.text_content.trim(),
                                                prefix
                                            ),
                                            self.current_location.clone(),
                                        );
                                        self.report_validation_error_to(
                                            err,
                                            &mut ev_state.error_codes,
                                        );
                                        ev_state.validity = SchemaValidity::Invalid;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            self.report_expression_warnings(raised);
                            self.report_validation_error_to(err, &mut ev_state.error_codes);
                            ev_state.validity = SchemaValidity::Invalid;
                        }
                    }
                }
            }

            // Clause 5.1.1: under an ·instance-specified type definition· the
            // value constraint must be a valid default for it (Element Default
            // Valid (Immediate), §3.3.6.2, clause 1: Simple Default Valid,
            // §3.2.6.2) — its {lexical form}, valid above, also "maps to
            // V.{value} in the value space of T".
            if overridden
                && ev_state.type_source == Some(TypeSource::XsiType)
                && ev_state.validity == SchemaValidity::Valid
            {
                self.check_default_maps_to_value(ev_state);
            }

            // Check fixed value on element — cvc-elt.5.2.2.2.2 (§3.3.4.3).
            // Use value-space comparison so that lexically-different but value-equivalent
            // forms (e.g. boolean "1" vs "true", float "1.0" vs "1.000", token whitespace)
            // are treated as matching. Reuses `ev_state.typed_value` parsed above
            // (line ~1778) to avoid a second parse of the same text content.
            if let Some(elem_key) = ev_state.element_decl {
                let schema_set: &'a SchemaSet = self.schema_set;
                let elem_data = &schema_set.arenas.elements[elem_key];
                if let Some(fixed) = &elem_data.fixed_value {
                    // Clause 5.2.2.2.2: "the ·actual value· of E is equal or
                    // identical to D.{value constraint}.{value}" — the
                    // declaration's {value}, read under its ·effective simple
                    // type definition· (§3.3.2.1), also when another type
                    // governs E (`xsi:type`, a type alternative). XSD 1.0's
                    // clause 5.2.2.2.2 instead: "the ·actual value· of the
                    // item must match the canonical lexical representation of
                    // the {value constraint} value" — a comparison by value,
                    // with no facet applied to that form
                    // (`xsd10_fixed_matches`). Under the declared type,
                    // reading the literal there gives that {value} (both
                    // versions). An untyped {value} (`xs:anySimpleType`
                    // declared: no primitive type of its own) is read under
                    // the governing type instead.
                    let declared = elem_data
                        .resolved_type
                        .unwrap_or_else(|| TypeKey::Complex(schema_set.any_type_key()));
                    let declared_value = ev_state
                        .typed_value
                        .as_ref()
                        .filter(|_| !substituted_fixed && ev_state.schema_type != Some(declared))
                        .and_then(|_| schema_set.value_constraint_value(elem_data))
                        .filter(|(_, value)| value.type_code != XmlTypeCode::UntypedAtomic);
                    let matches = if substituted_fixed {
                        true
                    } else if let (Some((lexical, value)), Some(typed)) =
                        (&declared_value, &ev_state.typed_value)
                    {
                        if schema_set.is_xsd10() {
                            super::simple::xsd10_fixed_matches(
                                typed,
                                &ev_state.text_content,
                                value,
                                lexical,
                                ev_state.schema_type,
                                schema_set.value_constraint_namespaces(elem_data.source.as_ref()),
                                schema_set,
                            )
                        } else {
                            super::simple::typed_literals_equal(
                                typed,
                                &ev_state.text_content,
                                value,
                                lexical,
                            )
                        }
                    } else if let Some(ref typed) = ev_state.typed_value {
                        // The fixed literal is read with the declaration's
                        // namespace bindings (an `xs:QName` value by
                        // expanded name).
                        super::simple::fixed_matches_typed_in(
                            &ev_state.text_content,
                            typed,
                            fixed,
                            self.schema_set
                                .value_constraint_namespaces(elem_data.source.as_ref()),
                            ev_state.schema_type,
                            self.schema_set,
                        )
                    } else {
                        super::simple::fixed_values_equal(
                            &ev_state.text_content,
                            fixed,
                            ev_state.schema_type,
                            self.schema_set,
                        )
                    };
                    if !matches {
                        let elem_name = self.schema_set.name_table.resolve(ev_state.local_name);
                        let err = errors::error(
                            "cvc-elt.5.2.2",
                            format!(
                                "Element '{}' has fixed value '{}' but actual value is '{}'",
                                elem_name, fixed, ev_state.text_content,
                            ),
                            self.current_location.clone(),
                        );
                        self.report_validation_error_to(err, &mut ev_state.error_codes);
                        ev_state.validity = SchemaValidity::Invalid;
                    }
                }
            }
        }

        // 2b. Fixed value check for mixed-content elements — cvc-elt.5.2.2.1 + 5.2.2.2.1 (§3.3.4.3).
        if ev_state.content_type == Some(ContentType::Mixed) && !ev_state.is_nil {
            if let Some(elem_key) = ev_state.element_decl {
                let elem_data = &self.schema_set.arenas.elements[elem_key];
                if let Some(ref fixed) = elem_data.fixed_value {
                    let elem_name = self.schema_set.name_table.resolve(ev_state.local_name);
                    if ev_state.has_element_children {
                        // cvc-elt.5.2.2.1: no element children when fixed value is present
                        let err = errors::error(
                            "cvc-elt.5.2.2.1",
                            format!(
                                "Element '{}' has fixed value '{}' but contains element children",
                                elem_name, fixed,
                            ),
                            self.current_location.clone(),
                        );
                        self.report_validation_error_to(err, &mut ev_state.error_codes);
                        ev_state.validity = SchemaValidity::Invalid;
                    } else if ev_state.has_text && ev_state.text_content != *fixed {
                        // cvc-elt.5.2.2.2.1: initial value (concatenated text) must match fixed
                        let err = errors::error(
                            "cvc-elt.5.2.2.2",
                            format!(
                                "Element '{}' has fixed value '{}' but actual value is '{}'",
                                elem_name, fixed, ev_state.text_content,
                            ),
                            self.current_location.clone(),
                        );
                        self.report_validation_error_to(err, &mut ev_state.error_codes);
                        ev_state.validity = SchemaValidity::Invalid;
                    }
                }
            }
        }
    }

    /// F2/B1 step 2c: XSD 1.1 assertion evaluation.
    ///
    /// Forwards the end-element event to the assertion fragment builder; when
    /// the outermost asserted element closes, finalizes the buffered fragment
    /// and evaluates all (deferred + current) assertions. Returns the
    /// `AssertionOutcome` for the PSVI, or `None` when the type has none.
    #[cfg(feature = "xsd11")]
    fn finish_assertions(
        &mut self,
        ev_state: &mut ElementValidationState,
    ) -> Option<AssertionOutcome> {
        let type_has_assertions = matches!(ev_state.schema_type,
            Some(TypeKey::Complex(ct_key)) if has_inherited_assertions(ct_key, &self.schema_set.arenas));
        #[cfg(feature = "xsd11")]
        let mut assertion_outcome: Option<AssertionOutcome> = None;

        #[cfg(feature = "xsd11")]
        'assertion_eval: {
            debug_assert!(
                self.flags.contains(ValidationFlags::PROCESS_ASSERTIONS)
                    == (self.assertion_source == AssertionSource::FragmentBuffer),
                "PROCESS_ASSERTIONS / AssertionSource invariant violated at end-element"
            );

            if !self.is_buffering_assertions() {
                // Clean up orphan fragment_builder left by force_start when
                // the CTA element's type had no assertions and cta_switched
                // was false (so redetect_assertions_after_cta was never called).
                if self.fragment_builder.is_some() {
                    self.fragment_builder.take();
                    self.pending_assertion_frames.clear();
                    if let Some(arena) = self.fragment_arena.as_mut() {
                        arena.reset();
                    }
                }
                if type_has_assertions {
                    assertion_outcome = Some(AssertionOutcome::NotEvaluated);
                }
                break 'assertion_eval;
            }

            // Forward end_element to builder. A valid element with simple
            // content has its [schema normalized value] as its text (XDM 1.0
            // §6.2.4), as in a typed document.
            if let Some(builder) = self.fragment_builder.as_mut() {
                if let (SchemaValidity::Valid, Some(ContentType::TextOnly), Some(type_key), false) = (
                    ev_state.validity,
                    ev_state.content_type,
                    ev_state.schema_type,
                    ev_state.is_nil,
                ) {
                    if builder.has_only_pending_text() {
                        let normalized = super::simple::schema_normalized(
                            builder.pending_text(),
                            type_key,
                            self.schema_set,
                        );
                        if let Some(normalized) = normalized {
                            builder.replace_pending_text(&normalized);
                        }
                    }
                }
                if let Err(e) = builder.end_element() {
                    self.abort_assertion_buffer_op("end_element", e);
                    ev_state.validity = SchemaValidity::Invalid;
                    break 'assertion_eval;
                }
            }

            if ev_state.owns_assertion_buffer {
                // Pop the assertion frame for this element
                let frame = match self.assertion_buffer_stack.pop() {
                    Some(f) => f,
                    None => {
                        // Should not happen, but don't panic in validation
                        self.abort_assertion_buffering(
                            "Internal: assertion buffer stack underflow".into(),
                        );
                        ev_state.validity = SchemaValidity::Invalid;
                        break 'assertion_eval;
                    }
                };

                if self.assertion_buffer_stack.is_empty() {
                    // Outermost asserted element closes — finalize and evaluate
                    let builder = match self.fragment_builder.take() {
                        Some(b) => b,
                        None => {
                            // builder was already taken (should not happen)
                            self.pending_assertion_frames.clear();
                            if let Some(arena) = self.fragment_arena.as_mut() {
                                arena.reset();
                            }
                            break 'assertion_eval;
                        }
                    };
                    match builder.finalize() {
                        Ok(doc) => {
                            // Evaluate nested (deferred) frames first
                            let pending = std::mem::take(&mut self.pending_assertion_frames);
                            for pf in &pending {
                                let (errs, raised) = self.collect_expression_warnings(|| {
                                    evaluate_complex_type_assertions(
                                        &doc,
                                        pf.element_ref,
                                        pf.complex_type_key,
                                        self.schema_set,
                                    )
                                });
                                self.report_expression_warnings_deferred(
                                    raised,
                                    &pf.context,
                                    &pf.location,
                                );
                                // The nested element is invalid; so is every
                                // strictly assessed ancestor up to this one
                                // (§3.3.5.1 `[validity]` clause 1.1.2). Those in
                                // between have already closed, so the verdict
                                // lands here and travels on from here.
                                if !errs.is_empty() && pf.propagates_to_outer {
                                    invalidate(&mut ev_state.validity);
                                }
                                self.report_assertion_errors_deferred(
                                    errs,
                                    &pf.context,
                                    &pf.location,
                                );
                            }
                            // Evaluate outermost frame (the current path is its own)
                            let (errs, raised) = self.collect_expression_warnings(|| {
                                evaluate_complex_type_assertions(
                                    &doc,
                                    frame.element_ref,
                                    frame.complex_type_key,
                                    self.schema_set,
                                )
                            });
                            self.report_expression_warnings(raised);
                            if errs.is_empty() {
                                assertion_outcome = Some(AssertionOutcome::Passed);
                            } else {
                                assertion_outcome = Some(AssertionOutcome::Failed);
                            }
                            self.report_assertion_errors(errs, ev_state);
                        }
                        Err(e) => {
                            // Clear stale deferred frames — they reference
                            // nodes in the failed document and must not leak
                            // into the next buffered subtree.
                            self.pending_assertion_frames.clear();
                            let err = errors::error(
                                "cvc-assertion",
                                format!("Failed to finalize assertion fragment: {}", e),
                                self.current_location.clone(),
                            );
                            self.report_validation_error_to(err, &mut ev_state.error_codes);
                            ev_state.validity = SchemaValidity::Invalid;
                            assertion_outcome = Some(AssertionOutcome::Failed);
                        }
                    }
                    // Reset arena for reuse
                    if let Some(arena) = self.fragment_arena.as_mut() {
                        arena.reset();
                    }
                } else {
                    // Nested asserted element — defer to outermost close
                    let mut deferred = frame;
                    deferred.context = self.capture_context();
                    deferred.location = self.current_location.clone();
                    deferred.propagates_to_outer = self.nested_failure_reaches_outer();
                    self.pending_assertion_frames.push(deferred);
                    assertion_outcome = Some(AssertionOutcome::NotEvaluated);
                }
            }
        }

        // Fallback: if assertion_outcome wasn't set but the type has assertions
        #[cfg(feature = "xsd11")]
        if assertion_outcome.is_none() && type_has_assertions {
            assertion_outcome = Some(AssertionOutcome::NotEvaluated);
        }

        assertion_outcome
    }

    /// F2/B1 step 3: identity-constraint scope teardown.
    ///
    /// Records selector/field values for this element, checks the keyrefs whose
    /// scope it is against its own [identity-constraint table], pops the IC
    /// scope table and propagates key/unique tables to the parent (or saves
    /// the root tables).
    fn finish_identity_constraints(&mut self, ev_state: &mut ElementValidationState) {
        // 3. Identity constraint processing (field values + scope exit + keyref cross-ref)
        // §3.11.4 Identity-constraint Satisfied clause 3: a field node must have
        // a simple type or a complex type with simple content. Element-only,
        // mixed, AND empty content types all lack a simple {variety} — an
        // attribute-only complex type (ContentType::Empty) is equally invalid
        // as a field value (msData idH006).
        let is_complex_content = matches!(
            ev_state.content_type,
            Some(ContentType::ElementOnly) | Some(ContentType::Mixed) | Some(ContentType::Empty)
        );
        // Resolve QName/NOTATION typed values for IC comparison (namespace-aware).
        // Use a separate copy to preserve the original PSVI typed_value.
        let ic_typed_value = Self::resolve_ic_qname_value(
            &ev_state.typed_value,
            &ev_state.text_content,
            ev_state.ns_context.as_ref(),
            &self.schema_set.name_table,
        );
        let ic_ref = ic_typed_value.as_ref().or(ev_state.typed_value.as_ref());
        self.process_constraints_end_element(
            &ev_state.text_content,
            ic_ref,
            ev_state.is_nil,
            is_complex_content,
            &mut ev_state.error_codes,
            &mut ev_state.validity,
        );

        // 3b. Pop scope table and propagate key/unique tables upward to parent
        // (§3.11.5 [node table] clause 1: the entries of the children's tables).
        if let Some(Some(scope_map)) = self.ic_scope_tables.pop() {
            if let Some(parent_slot) = self.ic_scope_tables.last_mut() {
                let parent_map = parent_slot.get_or_insert_with(HashMap::new);
                for (ic_key, key_table) in scope_map {
                    match parent_map.entry(ic_key) {
                        std::collections::hash_map::Entry::Occupied(mut existing) => {
                            existing.get_mut().append(key_table)
                        }
                        std::collections::hash_map::Entry::Vacant(slot) => {
                            slot.insert(key_table);
                        }
                    }
                }
            } else {
                // Root element — save for public access via identity_constraint_tables()
                self.final_ic_tables = Some(
                    scope_map
                        .into_iter()
                        .map(|(ic_key, table)| (ic_key, table.into_table()))
                        .collect(),
                );
            }
        }
    }

    /// F2/B1 step 4: ID/IDREF/ENTITY bookkeeping.
    ///
    /// Collects ID/IDREF bindings from this element's typed value — owner is the
    /// parent per §3.17.5.2 — and checks that ENTITY values are declared.
    fn finish_id_bindings(&mut self, ev_state: &mut ElementValidationState) {
        // 4. ID/IDREF collection from element text content.
        // Owner is the parent element per §3.17.5.2: the binding is to the
        // element that has the ID-typed child in its [children].
        // ev_state is the popped child; the parent is now the stack top.
        if let Some(ref tv) = ev_state.typed_value {
            let parent_serial = self
                .validation_stack
                .last()
                .map(|e| e.element_serial)
                .unwrap_or(ev_state.element_serial); // root: no parent, use self
            self.collect_id_idref(tv, &ev_state.text_content, parent_serial);
            if self.check_entity_declared(tv) {
                // The element's own content is not String Valid (§3.16.4).
                invalidate(&mut ev_state.validity);
            }
        }
    }

    /// F2/B1 step 4b: the ID/IDREF verdict on the validation root, applied
    /// when the root closes (no element is left on the stack).
    ///
    /// Element Locally Valid (Element) (§3.3.4.3) clause 7: "If E is the
    /// ·validation root·, then it is ·valid· per Validation Root Valid
    /// (ID/IDREF) (§3.3.4.5)", which requires "1 There is no ID/IDREF binding
    /// in E.\[ID/IDREF table\] whose \[binding\] is the empty set. 2 There is
    /// no ID/IDREF binding in E.\[ID/IDREF table\] whose \[binding\] has more
    /// than one member." A duplicate ID (cvc-id.2) is reported where it occurs; an
    /// unresolved IDREF (cvc-id.1) is still reported by
    /// [`end_validation`](Self::end_validation) — only the root's
    /// `[validity]` is decided here, before the root's end event returns it.
    fn finish_validation_root_ids(&mut self, root: &mut ElementValidationState) {
        let start = self.root_idref_start.min(self.pending_idrefs.len());
        let dangling_idref = self.pending_idrefs[start..]
            .iter()
            .any(|(idref, _, _)| !self.id_values.contains_key(idref));
        if self.root_id_conflict || dangling_idref {
            invalidate(&mut root.validity);
        }
        self.root_id_conflict = false;
        self.root_idref_start = self.pending_idrefs.len();
    }

    /// F2/B1 step 5: PSVI assembly (§3.3.5.1).
    ///
    /// Pops the element path, records `[validity]` and the `[validation
    /// attempted]` property, propagates this element's assessment to the
    /// parent, and builds the final `SchemaInfo`.
    /// PSVI element-close side effects shared by the value-returning
    /// ([`assemble_schema_info`](Self::assemble_schema_info)) and value-free
    /// ([`validate_end_element_novalue`](Self::validate_end_element_novalue))
    /// end-element paths: pop the element path, advance the state machine,
    /// compute `[validation attempted]` (§3.3.5.1) and propagate it — and an
    /// invalid `[validity]` — to the parent. Returns the computed `[validation
    /// attempted]` so the value path can place it in the `SchemaInfo`.
    fn finalize_element(&mut self, ev_state: &ElementValidationState) -> ValidationAttempted {
        // 5. Update element path
        self.pop_element_path();
        self.current_state = ValidatorState::EndElement;

        // Compute [validation attempted] per spec §3.3.5.1
        let all_full = !ev_state.any_child_not_full && !ev_state.any_attr_not_full;
        let all_none = !ev_state.any_child_not_none && !ev_state.any_attr_not_none;
        let validation_attempted = if ev_state.strictly_assessed && all_full {
            ValidationAttempted::Full
        } else if !ev_state.strictly_assessed && all_none {
            ValidationAttempted::None
        } else {
            ValidationAttempted::Partial
        };

        // §3.3.5.1 `[validity]`: a strictly assessed parent is valid only if
        // "1.1.2 Neither its [children] nor its [attributes] contains an
        // information item (element or attribute respectively) whose [validity]
        // is invalid" and "1.1.3 … contains [no] information item … which is
        // ·attributed· to a strict ·wildcard particle· and whose [validity] is
        // notKnown". This child counts as invalid when it was strictly assessed
        // and found invalid, or when it was assessed under `strict` without a
        // governing declaration or type (an undeclared element under a strict
        // wildcard, cvc-elt.1 — clause 1.1.3). A laxly assessed child is
        // `notKnown` (clause 2), never invalid, even if this runtime marked it
        // invalid for, e.g., a bad attribute.
        let invalidates_parent = ev_state.validity == SchemaValidity::Invalid
            && (ev_state.strictly_assessed
                || ev_state.process_contents == ContentProcessing::Strict);

        // Propagate to parent
        if let Some(parent) = self.validation_stack.last_mut() {
            match validation_attempted {
                ValidationAttempted::Full => {
                    parent.any_child_not_none = true;
                }
                ValidationAttempted::None => {
                    parent.any_child_not_full = true;
                }
                ValidationAttempted::Partial => {
                    parent.any_child_not_full = true;
                    parent.any_child_not_none = true;
                }
            }
            // Only a strictly assessed parent can turn invalid (clause 1.2);
            // a laxly assessed one stays `notKnown` (clause 2).
            if invalidates_parent && parent.strictly_assessed {
                invalidate(&mut parent.validity);
            }
        }

        validation_attempted
    }

    fn assemble_schema_info(
        &mut self,
        ev_state: &mut ElementValidationState,
        #[cfg(feature = "xsd11")] assertion_outcome: Option<AssertionOutcome>,
    ) -> SchemaInfo {
        let validation_attempted = self.finalize_element(ev_state);
        let validity = assessed_validity(ev_state);

        SchemaInfo {
            element_decl: ev_state.element_decl,
            attribute_decl: None,
            schema_type: ev_state.schema_type,
            member_type: ev_state.member_type,
            validity,
            validation_attempted,
            is_default: ev_state.is_default,
            // [nil] (§3.3.5.3): "true if clause 3.2.3 of Element Locally
            // Valid (Element) … is satisfied" — not for a nilled element with
            // character or element [children], whose clause 3.2.3.1 failed
            // (`cvc-elt.3.2.1`), nor for one whose declaration has a fixed
            // value, clause 3.2.3.2 (`cvc-elt.3.2.2`); both are recorded on
            // the element's own codes.
            is_nil: ev_state.is_nil
                && !ev_state
                    .error_codes
                    .iter()
                    .any(|c| matches!(*c, "cvc-elt.3.2.1" | "cvc-elt.3.2.2")),
            content_type: ev_state.content_type,
            typed_value: ev_state.typed_value.take(),
            normalized_value: ev_state.normalized_value.take(),
            schema_error_codes: std::mem::take(&mut ev_state.error_codes),
            notation: ev_state.notation,
            deferred_by_cta: false,
            type_source: ev_state.type_source,
            #[cfg(feature = "xsd11")]
            cta_selected: ev_state.cta_selected,
            #[cfg(feature = "xsd11")]
            assertion_outcome,
        }
    }

    /// Finalize validation
    ///
    /// Checks that the validation stack is empty and performs IDREF validation.
    pub fn end_validation(&mut self) -> Result<(), ValidationError> {
        self.continue_run();
        // An operational failure is terminal: report it as the completion
        // error regardless of the state machine (the run was abandoned where
        // it happened, so the stack is not expected to be empty).
        if let Some(err) = self.operational_failure.clone() {
            self.current_state = ValidatorState::Finish;
            return Err(err);
        }
        if !self.current_state.can_finish() {
            return Err(errors::error(
                "cvc-complex-type",
                format!(
                    "end_validation called in invalid state {:?}",
                    self.current_state
                ),
                self.current_location.clone(),
            ));
        }

        if !self.validation_stack.is_empty() {
            return Err(errors::error(
                "cvc-complex-type",
                format!(
                    "Validation ended with {} unclosed elements",
                    self.validation_stack.len()
                ),
                self.current_location.clone(),
            ));
        }

        // IDREF validation (cvc-id.1). Check before every iteration and after
        // each callback: the first refusal must stop even this deferred batch.
        for index in 0..self.pending_idrefs.len() {
            if !self.continue_run() {
                return Err(self.operational_failure.clone().expect("terminal stop"));
            }
            let (idref_value, location, context) = &self.pending_idrefs[index];
            if !self.id_values.contains_key(idref_value) {
                let err = errors::error(
                    "cvc-id.1",
                    format!(
                        "IDREF '{}' does not match any ID in the document",
                        idref_value
                    ),
                    location.clone(),
                );
                let err = context.attach(err, true);
                self.dispatch_error(err);
                if let Some(err) = self.operational_failure.clone() {
                    return Err(err);
                }
            }
        }

        #[cfg(feature = "xsd11")]
        {
            debug_assert!(
                self.assertion_buffer_stack.is_empty(),
                "assertion_buffer_stack not empty at end_validation"
            );
            debug_assert!(
                self.fragment_builder.is_none(),
                "fragment_builder not None at end_validation"
            );
            debug_assert!(
                self.pending_assertion_frames.is_empty(),
                "pending_assertion_frames not empty at end_validation"
            );
        }

        self.current_state = ValidatorState::Finish;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Query API
    // -----------------------------------------------------------------------

    /// Get elements expected at the current position in the content model
    pub fn get_expected_elements(&self) -> Vec<ExpectedElement> {
        let ev_state = match self.validation_stack.last() {
            Some(s) => s,
            None => return Vec::new(),
        };

        match &ev_state.content_state {
            ContentValidatorState::Nfa {
                nfa, active_states, ..
            } => active_states
                .expected_element_terms(nfa)
                .into_iter()
                .map(|(name, namespace, element_key)| ExpectedElement {
                    local_name: name,
                    namespace,
                    element_key,
                })
                .collect(),
            ContentValidatorState::AllGroup { model, state, .. } => {
                let mut result = Vec::new();
                for (i, particle) in model.particles.iter().enumerate() {
                    if state.can_accept(model, i) {
                        if let crate::compiler::NfaTerm::Element {
                            ref name,
                            ref namespace,
                            ref element_key,
                            ..
                        } = particle.term
                        {
                            result.push(ExpectedElement {
                                local_name: *name,
                                namespace: *namespace,
                                element_key: *element_key,
                            });
                        }
                    }
                }
                result
            }
            _ => Vec::new(),
        }
    }

    /// The attributes expected or allowed on the current element: the
    /// {attribute uses} of its complex type (Structures §3.4.2.4) — its own
    /// `<attribute>` uses, those of its attribute groups, and those inherited
    /// from its base types, except, in a restriction, those whose names the
    /// restriction includes itself or prohibits — each name once, in that
    /// order, with whether the use is required. A prohibited `<attribute>` is
    /// no use. The type's attribute wildcard is not listed. Empty when the
    /// element has no complex type (simple type, skipped, or not yet
    /// validated).
    pub fn get_expected_attributes(&self) -> Vec<ExpectedAttribute> {
        let ev_state = match self.validation_stack.last() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let ct_key = match ev_state.schema_type {
            Some(TypeKey::Complex(ct)) => ct,
            _ => return Vec::new(),
        };

        let mut result = Vec::new();
        self.for_each_attribute_use(ct_key, |u| {
            result.push(ExpectedAttribute {
                local_name: u.name,
                namespace: u.namespace,
                attribute_key: u.attr_key,
                required: u.use_kind == AttributeUseKind::Required,
            });
        });
        result
    }

    /// The defaulted attributes of the current element — Structures §3.4.4.2:
    /// every attribute use U of the element's complex type T with
    /// "U.{required} = false", a non-·absent· ·effective value constraint·
    /// (`default` or `fixed`; the use's own, otherwise its declaration's), and
    /// no attribute of the element matching it. §3.4.5.1 adds each to the
    /// element's \[attributes\] with that constraint's lexical form.
    ///
    /// T.{attribute uses} (§3.4.2.4) are T's own `<attribute>` uses — local
    /// and global declarations alike —, those of its attribute groups
    /// (including an XSD 1.1 `defaultAttributes` group), and the uses
    /// inherited from its base types, except, in a restriction, those whose
    /// names the restriction includes itself or prohibits. A prohibition only
    /// stops inheritance: it never hides a use of the type itself. Empty when
    /// the element has no complex type (simple type, skipped, or not yet
    /// validated).
    pub fn get_default_attributes(&self) -> Vec<DefaultAttribute> {
        let ev_state = match self.validation_stack.last() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let ct_key = match ev_state.schema_type {
            Some(TypeKey::Complex(ct)) => ct,
            _ => return Vec::new(),
        };

        let mut result = Vec::new();
        self.for_each_attribute_use(ct_key, |u| {
            if u.use_kind != AttributeUseKind::Optional
                || ev_state.seen_attributes.contains(&(u.namespace, u.name))
            {
                return;
            }
            if let Some(value) = u.value {
                result.push(DefaultAttribute {
                    local_name: u.name,
                    namespace: u.namespace,
                    attribute_key: u.attr_key,
                    type_key: u.type_key,
                    value: value.to_string(),
                    typed_value: u
                        .type_key
                        .filter(|&tk| self.default_typed_value_wanted(tk))
                        .and_then(|tk| {
                            self.value_constraint_typed_value(value, tk, u.value_source)
                        }),
                });
            }
        });
        result
    }

    /// Get the `[inherited attributes]` PSVI property for the current element
    /// (XSD 1.1 §3.3.5.6, structures.html line 5200).
    ///
    /// Returns the frozen `incoming_inherited` snapshot — ancestor-owned
    /// potentially-inherited attributes with nearest-owner shadowing.
    /// This is the spec's `[inherited attributes]` property; it is NOT
    /// filtered by the element's own `[attributes]`.
    ///
    /// Returns empty for skipped elements.
    #[cfg(feature = "xsd11")]
    pub fn get_inherited_attributes(&self) -> Vec<InheritedAttribute> {
        let ev_state = match self.validation_stack.last() {
            Some(s) => s,
            None => return Vec::new(),
        };

        // §3.3.5.6: [inherited attributes] is only defined for non-skipped elements
        if ev_state.process_contents == ContentProcessing::Skip {
            return Vec::new();
        }

        let mut result = Vec::new();
        for ((ns, name), val) in &ev_state.incoming_inherited {
            result.push(InheritedAttribute {
                local_name: *name,
                namespace: *ns,
                attribute_key: val.attribute_key,
                value: val.value.clone(),
            });
        }
        result
    }

    /// Get the content processing mode for the current element
    pub fn content_processing(&self) -> ContentProcessing {
        self.validation_stack
            .last()
            .map(|s| s.process_contents)
            .unwrap_or(ContentProcessing::Strict)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Obtain an `ElementValidationState` for a newly opened element, reusing a
    /// recycled shell from `state_pool` (resetting it) when one is available,
    /// else allocating a fresh one. The returned state is equivalent to
    /// `ElementValidationState::new(local_name, namespace)` in every observable
    /// field; only heap capacity is carried over.
    fn acquire_state(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
    ) -> ElementValidationState {
        match self.state_pool.pop() {
            Some(mut state) => {
                state.reset(local_name, namespace);
                state
            }
            None => ElementValidationState::new(local_name, namespace),
        }
    }

    /// Push a new element onto the validation stack and update the element path
    fn push_element(&mut self, mut ev_state: ElementValidationState) {
        // Assign a unique serial for ID/IDREF owner-element binding (§3.17.5.2)
        ev_state.element_serial = self.next_element_serial;
        self.next_element_serial += 1;

        // Record the first element using this namespace (XSD 1.0 §4.3.2 Rule 4
        // "late-arriving components"). Gated to XSD 1.0; XSD 1.1 §G.1.15
        // relaxes the rule entirely, so the map stays empty there.
        if self.schema_set.is_xsd10() {
            self.first_namespace_use
                .entry(ev_state.namespace)
                .or_insert(ev_state.element_serial);
        }

        // The path step: count the element among the same-named siblings
        // opened so far under its parent (path depth `depth`).
        let depth = self.path_steps.len();
        if self.sibling_counts.len() <= depth {
            self.sibling_counts.push(SiblingCounter::default());
        }
        let position = self.sibling_counts[depth].next(ev_state.namespace, ev_state.local_name);
        self.path_steps.push(RawPathStep {
            namespace: ev_state.namespace,
            local_name: ev_state.local_name,
            position,
        });

        // Inherit base URI from parent element, or from the document-level
        // base URI for the root element.
        ev_state.base_uri = match self.validation_stack.last() {
            Some(parent) => parent.base_uri.clone(),
            None => self.instance_base_uri.clone(),
        };
        ev_state.schema_location_hint_start = self.schema_location_hints.len();
        ev_state.no_namespace_schema_location_hint_start =
            self.no_namespace_schema_location_hints.len();

        self.validation_stack.push(ev_state);
        self.ic_scope_tables.push(None);

        // Propagate inherited attributes from parent to child (XSD 1.1).
        // §3.3.5.6: [inherited attributes] only exists when the parent is
        // strictly/laxly assessed and the child is not attributed to a skip
        // wildcard.
        //
        // Parent's outgoing_inherited becomes the child's incoming_inherited
        // (frozen PSVI view) and outgoing_inherited (mutable for this
        // element's own inheritable attrs to shadow).
        //
        // Skipped entirely when the schema has no inheritable attributes (the
        // common case): the maps are then always empty and empty→empty
        // propagation is the identity. The `!is_empty()` guard makes the same
        // short-circuit hold dynamically for the (inheritable-declaring) schemas
        // whose current parent simply carries nothing inheritable.
        #[cfg(feature = "xsd11")]
        if self.has_inheritable_attributes {
            let len = self.validation_stack.len();
            if len >= 2 {
                let parent_pc = self.validation_stack[len - 2].process_contents;
                let child_pc = self.validation_stack[len - 1].process_contents;
                if parent_pc != ContentProcessing::Skip
                    && child_pc != ContentProcessing::Skip
                    && !self.validation_stack[len - 2].outgoing_inherited.is_empty()
                {
                    let from_parent = self.validation_stack[len - 2].outgoing_inherited.clone();
                    self.validation_stack[len - 1].incoming_inherited = from_parent.clone();
                    self.validation_stack[len - 1].outgoing_inherited = from_parent;
                }
            }
        }

        if self.current_state == ValidatorState::None {
            self.current_state = ValidatorState::Start;
        }
        self.current_state = ValidatorState::Element;
    }

    /// Pop the last element from the element path
    fn pop_element_path(&mut self) {
        if self.path_steps.pop().is_some() {
            // The closed element's children were counted one level down; its
            // next same-level sibling starts that level afresh.
            if let Some(children) = self.sibling_counts.get_mut(self.path_steps.len() + 1) {
                children.clear();
            }
            if self.path_nodes.len() > self.path_steps.len() {
                self.path_nodes.truncate(self.path_steps.len());
            }
        }
    }

    /// The name `id` as a string shared by all diagnostics of the run.
    fn shared_name(&mut self, id: NameId) -> Arc<str> {
        let schema_set = self.schema_set;
        self.name_cache
            .entry(id)
            .or_insert_with(|| Arc::from(schema_set.name_table.resolve_ref(id)))
            .clone()
    }

    fn shared_expanded_name(
        &mut self,
        namespace: Option<NameId>,
        local_name: NameId,
    ) -> ExpandedName {
        let namespace = namespace.map(|ns| self.shared_name(ns));
        ExpandedName::from_shared(namespace, self.shared_name(local_name))
    }

    /// The path to the current element, sharing its steps with every
    /// diagnostic made under it (built for the levels not built yet); `None`
    /// outside any element.
    fn current_element_path(&mut self) -> Option<InstancePath> {
        while self.path_nodes.len() < self.path_steps.len() {
            let step = self.path_steps[self.path_nodes.len()];
            let name = self.shared_expanded_name(step.namespace, step.local_name);
            let parent = self.path_nodes.last().cloned().unwrap_or_default();
            self.path_nodes
                .push(parent.child(PathStep::element(name, step.position)));
        }
        self.path_nodes.last().cloned()
    }

    /// The subject and the path of the current item — the attribute being
    /// validated, else the current element; `None` outside any element.
    fn current_item(&mut self) -> Option<(ValidationSubject, InstancePath)> {
        let element = self.current_element_path()?;
        Some(match self.current_attribute {
            Some((namespace, local_name)) => {
                let name = self.shared_expanded_name(namespace, local_name);
                let path = element.child(PathStep::attribute(name.clone()));
                (ValidationSubject::Attribute(name), path)
            }
            None => {
                let name = element.last()?.name().clone();
                (ValidationSubject::Element(name), element)
            }
        })
    }

    /// The current item as an [`ErrorContext`], for a report made later.
    fn capture_context(&mut self) -> ErrorContext {
        ErrorContext {
            item: self.current_item(),
        }
    }

    /// Give `err` the subject and path of the current item, unless it names
    /// one already.
    fn with_current_context(&mut self, err: ValidationError) -> ValidationError {
        if err.subject().is_some() || err.instance_path().is_some() {
            return err;
        }
        match self.current_item() {
            Some((subject, path)) => err.with_subject(subject).with_instance_path(path),
            None => err,
        }
    }

    /// [`with_current_context`](Self::with_current_context), and inside an
    /// element its local-name path as the error's `element_path` (written
    /// only if asked for), replacing any it had.
    fn with_current_context_and_path(&mut self, err: ValidationError) -> ValidationError {
        let mut err = self.with_current_context(err);
        if !self.path_steps.is_empty() {
            err.derive_element_path();
        }
        err
    }

    /// A warning about the current item.
    fn warning_here(&mut self, code: &'static str, message: String) -> ValidationWarning {
        let warning = ValidationWarning::new(code, message, self.current_location.clone());
        match self.current_item() {
            Some((subject, path)) => warning.with_subject(subject).with_instance_path(path),
            None => warning,
        }
    }

    /// Emit a validation error: record its constraint code on the current
    /// element's `error_codes` list and dispatch to the sink.
    fn emit_error(&mut self, err: ValidationError) {
        if !self.continue_run() {
            return;
        }
        let err = self.with_current_context(err);
        if let Some(ev) = self.validation_stack.last_mut() {
            ev.error_codes.push(err.constraint);
        }
        self.dispatch_error(err);
    }

    /// Emit a validation error recording the code on an explicit target
    /// instead of the stack top. Used during `validate_end_element` where
    /// the element has already been popped.
    fn emit_error_to(&mut self, err: ValidationError, codes: &mut Vec<&'static str>) {
        if !self.continue_run() {
            return;
        }
        let err = self.with_current_context(err);
        codes.push(err.constraint);
        self.dispatch_error(err);
    }

    /// Record a terminal operational failure out of band. It is not a document
    /// error and must never be offered to an exhausted or cancelled sink.
    fn fail_operational(&mut self, constraint: &'static str, message: String) {
        if self.operational_failure.is_none() {
            self.operational_failure = Some(self.enrich(errors::error(constraint, message, None)));
        }
    }

    /// Abandon this run for an operational reason. The first failure wins;
    /// all later diagnostic callbacks and push operations are inert, and
    /// `end_validation` returns the recorded failure. Existing findings remain
    /// in the sink. This does not declare the document schema-invalid.
    pub fn abort_run(&mut self, message: impl Into<String>) {
        self.fail_operational("validation-aborted", message.into());
    }

    /// Check cooperative control without reporting a diagnostic to the sink.
    fn continue_run(&mut self) -> bool {
        if self.operational_failure.is_some() {
            return false;
        }
        if self.sink.stop_requested() {
            self.abort_run("Validation stopped at the sink's request");
            return false;
        }
        true
    }

    fn dispatch_error(&mut self, err: ValidationError) {
        if self.continue_run() {
            self.sink.on_error(err);
            self.continue_run();
        }
    }

    fn dispatch_warning(&mut self, warning: ValidationWarning) {
        if self.continue_run() {
            self.sink.on_warning(warning);
            self.continue_run();
        }
    }

    /// The terminal operational failure recorded for this run, if any.
    ///
    /// Operational failures — a counted content model exceeding an execution
    /// limit (`validation-resource-limit`, see
    /// [`ContentModelLimitExceeded`](crate::compiler::ContentModelLimitExceeded))
    /// cooperative sink/adapter cancellation (`validation-aborted`), or a content model that could not be prepared at load time
    /// (`validation-preparation-failed`) — abort assessment. They are not
    /// validity verdicts: the driver returns an error from completion instead
    /// of a `DriveOutcome`, and direct push callers get `Err` from
    /// [`end_validation`](Self::end_validation).
    pub fn operational_failure(&self) -> Option<&ValidationError> {
        self.operational_failure.as_ref()
    }

    /// Raise `validation-preparation-failed` for a complex type whose content
    /// model is missing from the prepared map.
    fn fail_content_model_unavailable(&mut self, ct_key: ComplexTypeKey) {
        let reason = self
            .content_models
            .failures
            .get(&ct_key)
            .cloned()
            .unwrap_or_else(|| "no content model was prepared for this type".to_string());
        let type_label = self.complex_type_label(ct_key);
        self.fail_operational(
            "validation-preparation-failed",
            format!(
                "Content model of complex type {type_label} is unavailable: {reason}; \
                 assessment aborted"
            ),
        );
    }

    /// `{ns}local` / `local` / `(anonymous)` label for diagnostics.
    fn complex_type_label(&self, ct_key: ComplexTypeKey) -> String {
        let ct = &self.schema_set.arenas.complex_types[ct_key];
        let names = &self.schema_set.name_table;
        match ct.name {
            Some(local) => match ct.target_namespace {
                Some(ns) => format!("{{{}}}{}", names.resolve(ns), names.resolve(local)),
                None => names.resolve(local).to_string(),
            },
            None => "(anonymous)".to_string(),
        }
    }

    fn report_native_error(
        &mut self,
        constraint: &'static str,
        message: impl Into<String>,
        cause: errors::NativeCause,
    ) {
        let mut err = errors::error(constraint, message, self.current_location.clone())
            .with_native_cause(cause);
        if let Some(ev) = self.validation_stack.last() {
            if let Some(key) = ev.schema_type {
                err = err.with_type_context(key, self.schema_set);
            }
            if let Some(key) = ev.element_decl {
                err =
                    err.with_component(errors::SchemaComponent::for_element(key, self.schema_set));
            }
        }
        self.report_validation_error(err);
    }

    fn report_error(&mut self, constraint: &'static str, message: impl Into<String>) {
        let err = errors::error(constraint, message, self.current_location.clone());
        let err = self.with_current_context_and_path(err);
        self.emit_error(err);
    }

    /// Enrich `err` with `current_location` and the local-name element path
    /// (if either is set). `with_location` / `with_path` overwrite so this is
    /// safe to call even when the error already carries them.
    fn enrich(&mut self, err: ValidationError) -> ValidationError {
        let err = match &self.current_location {
            Some(loc) => err.with_location(loc.clone()),
            None => err,
        };
        self.with_current_context_and_path(err)
    }

    /// Enrich `err` with current location/path and report it to an explicit
    /// `codes` target (e.g. during `validate_end_element` after the element
    /// has been popped off the stack).
    fn report_validation_error_to(&mut self, err: ValidationError, codes: &mut Vec<&'static str>) {
        let err = self.enrich(err);
        self.emit_error_to(err, codes);
    }

    /// Enrich an existing `ValidationError` with location/path and report it.
    fn report_validation_error(&mut self, err: ValidationError) {
        let err = self.enrich(err);
        self.emit_error(err);
    }

    /// Report an `xs:decimal` value that was validated exactly but is beyond
    /// what `rust_decimal::Decimal` holds, so its typed value is its canonical
    /// string. Datatypes 1.1 §5.4: such a value "must not be treated as invalid,
    /// and the unsupported value must not be quietly changed to a supported
    /// value", and the processor "must indicate to the user and/or downstream
    /// application that it cannot process the input data with assured
    /// correctness" — a warning, not a validity error.
    ///
    /// Reported only while [`ValidationFlags::REPORT_WARNINGS`] is set (the
    /// default); the typed value's shape (an `xs:decimal` type code with a
    /// string payload) tells a downstream consumer the same thing either way.
    fn note_decimal_capacity(&mut self, typed_value: &XmlValue) {
        if !self.flags.contains(ValidationFlags::REPORT_WARNINGS) {
            return;
        }
        let Some(lexical) = super::simple::capacity_decimal(typed_value) else {
            return;
        };
        let at = match self.current_element_path() {
            Some(path) => format!(" ({})", path.local_name_path()),
            None => String::new(),
        };
        let message = format!(
            "xs:decimal value '{lexical}'{at} is valid but exceeds the decimal capacity of \
             this implementation (a 96-bit mantissa with at most 28 fraction digits); \
             facets were checked on the exact value, and the typed value is carried as \
             its canonical string, so arithmetic on it raises FOAR0002"
        );
        let warning = self.warning_here(
            crate::types::exact_decimal::DECIMAL_CAPACITY_WARNING,
            message,
        );
        self.dispatch_warning(warning);
    }

    /// Report [`SECONDS_CAPACITY_WARNING`](crate::types::validators::SECONDS_CAPACITY_WARNING)
    /// for a date/time or duration value whose seconds have more fraction
    /// digits than this implementation holds: the value is valid; its facets,
    /// value constraints and identity constraints compare the literal as
    /// written, while its typed value holds the seconds truncated toward zero
    /// to the digits that fit — never rounded up to the next second — so a
    /// consumer of the typed value (the PSVI, an assertion or type
    /// alternative expression) sees the truncated value (Datatypes §5.4: the
    /// processor "must indicate to the user and/or downstream application that
    /// it cannot process the input data with assured correctness").
    ///
    /// Reported only while [`ValidationFlags::REPORT_WARNINGS`] is set.
    fn note_seconds_capacity(&mut self, typed_value: &XmlValue, text: &str) {
        if !self.flags.contains(ValidationFlags::REPORT_WARNINGS) {
            return;
        }
        let Some(lexical) = super::simple::capacity_seconds(typed_value, text) else {
            return;
        };
        let at = match self.current_element_path() {
            Some(path) => format!(" ({})", path.local_name_path()),
            None => String::new(),
        };
        let message = format!(
            "value '{lexical}'{at} is valid but its seconds have more fraction digits than \
             this implementation holds (at most 28); facets, value constraints and identity \
             constraints were checked on the value as written, but its typed value holds \
             the seconds truncated toward zero, so expressions on it do not distinguish it \
             from a value that differs only in the dropped digits"
        );
        let warning =
            self.warning_here(crate::types::validators::SECONDS_CAPACITY_WARNING, message);
        self.dispatch_warning(warning);
    }

    /// Run `validate` — a value's validation, or an assertion's or a type
    /// alternative's evaluation — and collect the warnings that the XSD 1.1
    /// expressions it evaluates raise (`expression_warnings`), when warnings
    /// are reported: an expression whose date/time or duration result keeps
    /// its seconds truncated reports
    /// [`SECONDS_CAPACITY_WARNING`](crate::types::validators::SECONDS_CAPACITY_WARNING)
    /// as a literal does ([`note_seconds_capacity`](Self::note_seconds_capacity)).
    fn collect_expression_warnings<R>(
        &self,
        validate: impl FnOnce() -> R,
    ) -> (R, Vec<ExpressionWarning>) {
        if cfg!(feature = "xsd11") && self.flags.contains(ValidationFlags::REPORT_WARNINGS) {
            expression_warnings::collecting(validate)
        } else {
            (validate(), Vec::new())
        }
    }

    /// Report the warnings [`collect_expression_warnings`](Self::collect_expression_warnings)
    /// collected, about the current item.
    fn report_expression_warnings(&mut self, warnings: Vec<ExpressionWarning>) {
        for ExpressionWarning {
            code,
            source,
            message,
        } in warnings
        {
            if !self.continue_run() {
                return;
            }
            let at = match self.current_element_path() {
                Some(path) => format!(" ({})", path.local_name_path()),
                None => String::new(),
            };
            let warning = self.warning_here(code, format!("{source}{at}: {message}"));
            self.dispatch_warning(warning);
        }
    }

    /// Report the warnings [`collect_expression_warnings`](Self::collect_expression_warnings)
    /// collected for a deferred (nested) assertion frame, about the item and
    /// at the location it recorded.
    #[cfg(feature = "xsd11")]
    fn report_expression_warnings_deferred(
        &mut self,
        warnings: Vec<ExpressionWarning>,
        context: &ErrorContext,
        location: &Option<SourceLocation>,
    ) {
        for ExpressionWarning {
            code,
            source,
            message,
        } in warnings
        {
            if !self.continue_run() {
                return;
            }
            let at = match &context.item {
                Some((_, path)) => format!(" ({})", path.local_name_path()),
                None => String::new(),
            };
            let warning =
                ValidationWarning::new(code, format!("{source}{at}: {message}"), location.clone());
            let warning = match &context.item {
                Some((subject, path)) => warning
                    .with_subject(subject.clone())
                    .with_instance_path(path.clone()),
                None => warning,
            };
            self.dispatch_warning(warning);
        }
    }

    /// Mark the current element as invalid.
    fn mark_current_invalid(&mut self) {
        if let Some(s) = self.validation_stack.last_mut() {
            s.validity = SchemaValidity::Invalid;
        }
    }

    /// [`invalidate`] the current element (stack top): a `valid` verdict
    /// turns `invalid`; `notKnown` stays (unlike
    /// [`mark_current_invalid`](Self::mark_current_invalid)).
    fn invalidate_current(&mut self) {
        if let Some(s) = self.validation_stack.last_mut() {
            invalidate(&mut s.validity);
        }
    }

    /// Get the effective base URI for the current element (or the document
    /// base URI if no element is on the stack).
    fn current_element_base_uri(&self) -> String {
        self.validation_stack
            .last()
            .map(|ev| ev.base_uri.clone())
            .unwrap_or_else(|| self.instance_base_uri.clone())
    }

    fn rebase_hint_range(&mut self, sl_start: usize, nnsl_start: usize, base_uri: &str) {
        for hint in &mut self.schema_location_hints[sl_start..] {
            hint.base_uri = base_uri.to_string();
        }
        for hint in &mut self.no_namespace_schema_location_hints[nnsl_start..] {
            hint.base_uri = base_uri.to_string();
        }
    }

    // -----------------------------------------------------------------------
    // XSI built-in attribute helpers
    // -----------------------------------------------------------------------

    /// Dispatch validation for an `xsi:*` attribute.
    ///
    /// The four built-in XSI attributes (`type`, `nil`, `schemaLocation`,
    /// `noNamespaceSchemaLocation`) are validated against their spec-defined
    /// types. Unknown `xsi:*` attributes fall through to normal
    /// attribute/wildcard validation.
    fn validate_xsi_attribute(&mut self, local_name: NameId, value: &str) -> SchemaInfo {
        let builtin = self.schema_set.builtin_types();
        if local_name == well_known::XSI_TYPE {
            // The value is an xs:QName (String Valid). The element's start
            // resolves it as its ·instance-specified type definition· and
            // reports one that resolves to nothing (cvc-elt.4.1).
            let type_key = TypeKey::Simple(builtin.qname);
            let attr_key = builtin.xsi_type_attr;
            let mut info = self.validate_xsi_simple_value(value, type_key, attr_key);
            // XSD 1.1 Attribute Locally Valid (§3.2.4.1): "5 If D is the
            // built-in declaration for xsi:type (Attribute Declaration for the
            // 'type' attribute (§3.2.7.1)), then A's ·actual value· ·resolves·
            // to a type definition." XSD 1.0's rule has no such clause
            // (clauses 1–4), so there the attribute stays valid. Reported here
            // only when the element's start did not report it already.
            if self.schema_set.is_xsd11()
                && info.validity == SchemaValidity::Valid
                && !self.xsi_type_resolves(value)
            {
                info.validity = SchemaValidity::Invalid;
                let serial = self.validation_stack.last().map(|ev| ev.element_serial);
                if serial.is_some() && self.xsi_type_unresolved_reported == serial {
                    info.schema_error_codes.push("cvc-attribute.5");
                } else {
                    self.report_error(
                        "cvc-attribute.5",
                        format!(
                            "Attribute xsi:type value '{}' does not resolve to a type definition",
                            value
                        ),
                    );
                }
            }
            return info;
        }
        if local_name == well_known::XSI_NIL {
            let type_key = TypeKey::Simple(builtin.boolean);
            let attr_key = builtin.xsi_nil_attr;
            return self.validate_xsi_simple_value(value, type_key, attr_key);
        }
        if local_name == well_known::XSI_SCHEMA_LOCATION {
            return self.validate_xsi_schema_location(value);
        }
        if local_name == well_known::XSI_NO_NAMESPACE_SCHEMA_LOCATION {
            return self.validate_xsi_no_ns_schema_location(value);
        }
        // Unknown xsi:* — fall through to normal attribute/wildcard validation
        self.validate_unknown_xsi_attribute(local_name, value)
    }

    /// Unknown `xsi:*` attributes go through normal wildcard validation.
    fn validate_unknown_xsi_attribute(&mut self, local_name: NameId, value: &str) -> SchemaInfo {
        let ct_key = match self.validation_stack.last() {
            Some(ev) => match ev.schema_type {
                Some(TypeKey::Complex(ct)) => ct,
                _ => return SchemaInfo::empty(),
            },
            None => return SchemaInfo::empty(),
        };
        self.validate_attribute_against_type(
            ct_key,
            local_name,
            Some(well_known::XSI_NAMESPACE),
            value,
        )
    }

    /// Validate a value against a built-in simple type and return attribute `SchemaInfo`.
    fn validate_xsi_simple_value(
        &mut self,
        value: &str,
        type_key: TypeKey,
        attr_key: AttributeKey,
    ) -> SchemaInfo {
        match self.validate_attribute_value(value, type_key) {
            Ok(result) => SchemaInfo {
                element_decl: None,
                attribute_decl: Some(attr_key),
                schema_type: Some(type_key),
                member_type: result.member_type,
                validity: SchemaValidity::Valid,
                validation_attempted: ValidationAttempted::Full,
                is_default: false,
                is_nil: false,
                content_type: None,
                typed_value: Some(result.typed_value),
                normalized_value: result.normalized_value,
                schema_error_codes: Vec::new(),
                notation: None,
                deferred_by_cta: false,
                type_source: None,
                #[cfg(feature = "xsd11")]
                cta_selected: false,
                #[cfg(feature = "xsd11")]
                assertion_outcome: None,
            },
            Err(err) => {
                self.report_validation_error(err);
                SchemaInfo {
                    attribute_decl: Some(attr_key),
                    schema_type: Some(type_key),
                    validity: SchemaValidity::Invalid,
                    validation_attempted: ValidationAttempted::Full,
                    ..SchemaInfo::empty()
                }
            }
        }
    }

    /// Validate `xsi:schemaLocation` as a whitespace-separated list of anyURI
    /// tokens with even token count.
    fn validate_xsi_schema_location(&mut self, value: &str) -> SchemaInfo {
        let builtin = self.schema_set.builtin_types();
        let any_uri_key = TypeKey::Simple(builtin.any_uri);
        let list_type_key = TypeKey::Simple(builtin.xsi_schema_location_type);
        let attr_key = builtin.xsi_schema_location_attr;

        let tokens: Vec<&str> = value.split_whitespace().collect();
        let mut validity = SchemaValidity::Valid;

        // Even token count check (namespace/location pairs)
        // `usize::is_multiple_of` was stabilized in Rust 1.87; the modulo keeps
        // the crate buildable on older toolchains.
        #[allow(clippy::manual_is_multiple_of)]
        if tokens.len() % 2 != 0 {
            self.report_error(
                "cvc-schema-location",
                format!(
                    "xsi:schemaLocation value must contain an even number of URI tokens, \
                     but found {} tokens",
                    tokens.len()
                ),
            );
            validity = SchemaValidity::Invalid;
        }

        // Validate each token as xs:anyURI
        for token in &tokens {
            if let Err(err) =
                super::simple::validate_simple_type(token, any_uri_key, self.schema_set)
            {
                self.report_validation_error(err);
                validity = SchemaValidity::Invalid;
            }
        }

        // Accumulate namespace/location pairs (even from invalid attributes —
        // the complete pairs are still valid hints).
        //
        // XSD 1.0 §4.3.2 Rule 4 ("late-arriving components") is enforced
        // here: a pair that announces a *new* schema document for a
        // namespace whose first item already appeared earlier in the
        // instance document is rejected. Re-announcing the same schema
        // document (same location) for an already-seen namespace is
        // allowed — no new components are introduced. XSD 1.1 §G.1.15
        // relaxes the rule entirely (gated by `is_xsd10()`).
        let base_uri = self.current_element_base_uri();
        let xsd10 = self.schema_set.is_xsd10();
        let current_serial = self.validation_stack.last().map(|ev| ev.element_serial);
        for pair in tokens.chunks_exact(2) {
            let ns_str = pair[0];
            let loc_str = pair[1];

            if xsd10 {
                if let Some(serial) = current_serial {
                    let ns_id: Option<NameId> = if ns_str.is_empty() {
                        None
                    } else {
                        Some(self.schema_set.name_table.add(ns_str))
                    };
                    if let Some(&first_serial) = self.first_namespace_use.get(&ns_id) {
                        if first_serial != serial
                            && !self
                                .schema_location_hints
                                .iter()
                                .any(|h| h.namespace == ns_str && h.location == loc_str)
                        {
                            self.report_error(
                                "sch-late-component",
                                format!(
                                    "xsi:schemaLocation announces a new schema document \
                                     '{loc_str}' for namespace '{ns_str}', but an item \
                                     from that namespace has already been encountered \
                                     earlier in the instance document (XSD 1.0 §4.3.2 \
                                     Rule 4 'late-arriving components')"
                                ),
                            );
                            validity = SchemaValidity::Invalid;
                        }
                    }
                }
            }

            self.schema_location_hints.push(SchemaLocationHint {
                namespace: ns_str.to_string(),
                location: loc_str.to_string(),
                base_uri: base_uri.clone(),
            });
        }

        SchemaInfo {
            element_decl: None,
            attribute_decl: Some(attr_key),
            schema_type: Some(list_type_key),
            member_type: None,
            validity,
            validation_attempted: ValidationAttempted::Full,
            is_default: false,
            is_nil: false,
            content_type: None,
            typed_value: None,
            normalized_value: if tokens.is_empty() {
                None
            } else {
                Some(tokens.join(" "))
            },
            schema_error_codes: Vec::new(),
            notation: None,
            deferred_by_cta: false,
            type_source: None,
            #[cfg(feature = "xsd11")]
            cta_selected: false,
            #[cfg(feature = "xsd11")]
            assertion_outcome: None,
        }
    }

    /// Validate `xsi:noNamespaceSchemaLocation` as `xs:anyURI`.
    fn validate_xsi_no_ns_schema_location(&mut self, value: &str) -> SchemaInfo {
        let builtin = self.schema_set.builtin_types();
        let any_uri_key = TypeKey::Simple(builtin.any_uri);
        let attr_key = builtin.xsi_no_namespace_schema_location_attr;
        let mut result = self.validate_xsi_simple_value(value, any_uri_key, attr_key);

        let trimmed = value.trim();
        if !trimmed.is_empty() {
            // XSD 1.0 §4.3.2 Rule 4 (late-arriving components): only fire the
            // rule when the announced location is *new* — re-announcing the
            // same noNamespace schema document already seen on an earlier
            // element does not introduce new components and is allowed.
            // Gated to XSD 1.0; XSD 1.1 §G.1.15 relaxes the rule.
            if self.schema_set.is_xsd10() {
                let current_serial = self.validation_stack.last().map(|ev| ev.element_serial);
                if let Some(serial) = current_serial {
                    if let Some(&first_serial) = self.first_namespace_use.get(&None) {
                        if first_serial != serial
                            && !self
                                .no_namespace_schema_location_hints
                                .iter()
                                .any(|h| h.location == trimmed)
                        {
                            self.report_error(
                                "sch-late-component",
                                format!(
                                    "xsi:noNamespaceSchemaLocation announces a new schema \
                                     document '{trimmed}' for the absent namespace, but \
                                     an item from the absent namespace has already been \
                                     encountered earlier in the instance document (XSD 1.0 \
                                     §4.3.2 Rule 4 'late-arriving components')"
                                ),
                            );
                            result.validity = SchemaValidity::Invalid;
                        }
                    }
                }
            }

            self.no_namespace_schema_location_hints
                .push(NoNamespaceSchemaLocationHint {
                    location: trimmed.to_string(),
                    base_uri: self.current_element_base_uri(),
                });
        }

        result
    }

    // -----------------------------------------------------------------------
    // Identity constraint helpers
    // -----------------------------------------------------------------------

    /// Resolve a keyref's `refer` target to a concrete `IdentityConstraintKey`.
    ///
    /// Scans the identity constraint arena for a Key or Unique constraint
    /// whose name and target namespace match the given values.
    fn resolve_refer_key(
        &self,
        refer_local_name: NameId,
        refer_ns: Option<NameId>,
    ) -> Option<IdentityConstraintKey> {
        for (key, ic_data) in &self.schema_set.arenas.identity_constraints {
            if ic_data.kind == IdentityKind::Keyref {
                continue;
            }
            if ic_data.name != refer_local_name {
                continue;
            }
            let ic_target_ns = ic_data
                .source
                .as_ref()
                .and_then(|s| self.schema_set.documents.get(s.doc_id as usize))
                .and_then(|d| d.target_namespace);
            if ic_target_ns == refer_ns {
                return Some(key);
            }
        }
        None
    }

    /// Lazily compile an identity constraint and cache it.
    /// Returns `true` if compilation succeeded (constraint is usable).
    fn ensure_compiled(&mut self, ic_key: IdentityConstraintKey) -> bool {
        if !self.continue_run() {
            return false;
        }
        if let Some(cached) = self.compiled_constraints.get(&ic_key) {
            return cached.is_some();
        }
        let ic_data = &self.schema_set.arenas.identity_constraints[ic_key];
        let doc = ic_data
            .source
            .as_ref()
            .and_then(|s| self.schema_set.documents.get(s.doc_id as usize));
        let schema_xpath_default_ns = doc.and_then(|d| d.xpath_default_namespace);
        let target_namespace = doc.and_then(|d| d.target_namespace);
        let ic_name = ic_data.name;
        match CompiledIdentityConstraint::compile(
            ic_data,
            ic_key,
            &self.schema_set.name_table,
            schema_xpath_default_ns,
            target_namespace,
            self.schema_set.xsd_version,
        ) {
            Ok(mut compiled) => {
                // Resolve refer_key for keyref constraints
                if compiled.kind == IdentityKind::Keyref {
                    if let Some(refer) = &compiled.refer {
                        let refer_ns = refer.namespace.or(compiled.target_namespace);
                        compiled.refer_key = self.resolve_refer_key(refer.local_name, refer_ns);
                        if compiled.refer_key.is_none() {
                            let name = self.schema_set.name_table.resolve(ic_name);
                            let refer_display = format_resolved_qname(
                                &self.schema_set.name_table,
                                refer_ns,
                                refer.local_name,
                            );
                            // About the schema, not an instance item: no
                            // location, subject or path.
                            self.dispatch_warning(ValidationWarning::new(
                                "cvc-identity-constraint",
                                format!(
                                    "Keyref '{}': could not resolve refer target '{}'",
                                    name, refer_display
                                ),
                                None,
                            ));
                        }
                    }
                }
                self.compiled_constraints.insert(ic_key, Some(compiled));
                true
            }
            Err(e) => {
                let name = self.schema_set.name_table.resolve(ic_name);
                // About the schema, not an instance item: no location,
                // subject or path.
                self.dispatch_warning(ValidationWarning::new(
                    "cvc-identity-constraint",
                    format!(
                        "Identity constraint '{}': XPath compilation failed: {}",
                        name, e
                    ),
                    None,
                ));
                self.compiled_constraints.insert(ic_key, None);
                false
            }
        }
    }

    /// Advance existing constraints for a start element, then activate new
    /// constraints from the element declaration (if any).
    fn advance_constraints_start_element(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        element_key: Option<ElementKey>,
    ) {
        self.advance_constraints_start_element_inner(local_name, namespace, element_key, false);
    }

    /// Variant of [`advance_constraints_start_element`] used when the element
    /// is inside a wildcard `processContents="skip"` subtree. Bumps depth
    /// tracking on existing IC selectors / fields without admitting new
    /// matches, and never activates element-attached identity constraints:
    /// per XSD 1.1 §3.11.4 (and the W3C wild101..103 fixtures), skipped
    /// content is outside the schema's validation scope.
    fn advance_constraints_start_element_skipped(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
    ) {
        self.advance_constraints_start_element_inner(local_name, namespace, None, true);
    }

    fn advance_constraints_start_element_inner(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        element_key: Option<ElementKey>,
        skipped: bool,
    ) {
        let ns = namespace.unwrap_or(NameId(0));

        // 1. Advance existing active constraints
        for cs in &mut self.active_constraints {
            if skipped {
                cs.start_element_skipped(local_name, ns);
            } else {
                cs.start_element(local_name, ns);
            }
        }

        // 2. Activate new constraints from element declaration (only when
        //    NOT in skip mode — skipped elements have no element_key).
        if !skipped {
            if let Some(ek) = element_key {
                let ic_keys: Vec<IdentityConstraintKey> = self.schema_set.arenas.elements[ek]
                    .identity_constraints
                    .clone();
                // The element was pushed before this call: it is the stack top.
                let owner_depth = self.validation_stack.len().saturating_sub(1);
                for ic_key in ic_keys {
                    if !self.continue_run() {
                        return;
                    }
                    if self.ensure_compiled(ic_key) {
                        let compiled = self.compiled_constraints[&ic_key].as_ref().unwrap();
                        let mut cs = ConstraintStruct::new(compiled);
                        cs.owner_depth = owner_depth;
                        cs.activate();
                        self.active_constraints.push(cs);
                    }
                }
            }
        }
    }

    /// Process identity constraints at element end: advance fields/selectors,
    /// deactivate finished constraints, and perform scope-local keyref
    /// cross-reference.
    ///
    /// `validity` is the closing element's `[validity]`: a violation of a
    /// constraint that element carries (its scope just closed, or a field or
    /// selector match finished here) makes it invalid (§3.3.4.3 clause 6).
    #[allow(clippy::too_many_arguments)]
    fn process_constraints_end_element(
        &mut self,
        text_content: &str,
        typed_value: Option<&XmlValue>,
        is_nil: bool,
        is_complex_content: bool,
        error_codes: &mut Vec<&'static str>,
        validity: &mut SchemaValidity,
    ) {
        // 1. Advance all constraints (field value collection + key sequence
        //    finalization). Gated on a non-empty active list so the common
        //    constraint-free element end neither clones the element path nor
        //    the location (P0 allocation gate, XSD_COMPILER_REWORK §12.1).
        //    Steps 2–3 below still run: they own the scope-stack teardown and
        //    are already no-ops when nothing deactivated.
        if !self.active_constraints.is_empty() {
            let name_table = &self.schema_set.name_table;
            let location = self.current_location.clone();
            let mut ic_errors = Vec::new();
            for cs in &mut self.active_constraints {
                // The element path is attached below, only to the errors
                // there are — not built for every element end.
                let errs = cs.end_element_with_text(
                    text_content,
                    typed_value,
                    is_nil,
                    is_complex_content,
                    name_table,
                    "",
                    location.clone(),
                );
                let owner_depth = cs.owner_depth;
                ic_errors.extend(errs.into_iter().map(|err| (err, owner_depth)));
            }
            if !ic_errors.is_empty() {
                for (err, owner_depth) in ic_errors {
                    if !self.continue_run() {
                        return;
                    }
                    // The placeholder `""` becomes the current element's path.
                    let err = if err.element_path() == Some("") {
                        self.with_current_context_and_path(err)
                    } else {
                        err
                    };
                    self.emit_error_to(err, error_codes);
                    self.invalidate_ic_owner(owner_depth, validity);
                }
            }
        }

        // 2. Collect deactivated constraints (constraints whose scope element just closed)
        let mut deactivated: Vec<ConstraintStruct> = Vec::new();
        let mut i = 0;
        while i < self.active_constraints.len() {
            if !self.active_constraints[i].is_active() {
                deactivated.push(self.active_constraints.swap_remove(i));
            } else {
                i += 1;
            }
        }

        // 3. Keyrefs against the closing element's [identity-constraint table]
        if !deactivated.is_empty() {
            let mut scope_keyrefs: Vec<(KeyTable, Option<IdentityConstraintKey>)> = Vec::new();

            // Deactivated constraints belong to the closing element (their
            // scope element): a keyref failure below is its violation.
            for cs in deactivated {
                if cs.key_table.table().kind == IdentityKind::Keyref {
                    // Extract the resolved refer_key from the compiled constraint
                    let refer_key = self
                        .compiled_constraints
                        .get(&cs.ic_key)
                        .and_then(|opt| opt.as_ref())
                        .and_then(|compiled| compiled.refer_key);
                    scope_keyrefs.push((cs.key_table.into_table(), refer_key));
                } else {
                    // Insert key/unique table into current scope
                    let scope_slot = self.ic_scope_tables.last_mut();
                    if let Some(slot) = scope_slot {
                        let scope_map = slot.get_or_insert_with(HashMap::new);
                        match scope_map.entry(cs.ic_key) {
                            std::collections::hash_map::Entry::Occupied(mut existing) => {
                                existing.get_mut().append(cs.key_table)
                            }
                            std::collections::hash_map::Entry::Vacant(slot) => {
                                slot.insert(cs.key_table);
                            }
                        }
                    }
                }
            }

            // §3.11.4 clause 4.3: "there is a ·node table· associated with the
            // {referenced key} in the [identity-constraint table] of E … and
            // there is an entry in that table whose ·key-sequence· is equal or
            // identical to the keyref member's ·key-sequence·". E's table is
            // the scope map of the closing element: its own key/unique tables
            // and those its children propagated (§3.11.5). A key on a sibling
            // or an ancestor is not in it — "only element information items
            // within the sub-tree rooted at the element information item being
            // ·validated· can be referenced successfully" — so a keyref member
            // finds no entry then.
            let name_table = &self.schema_set.name_table;
            for (keyref_table, refer_key) in scope_keyrefs {
                let errs = match refer_key {
                    None => {
                        // Unresolvable at run time (the schema check resolves
                        // every `refer`): report the keyref once.
                        let keyref_name = name_table.resolve(keyref_table.constraint_name);
                        let refer_display = self
                            .compiled_constraints
                            .get(&keyref_table.ic_key)
                            .and_then(|opt| opt.as_ref())
                            .and_then(|compiled| {
                                compiled.refer.as_ref().map(|refer| {
                                    let refer_ns = refer.namespace.or(compiled.target_namespace);
                                    format_resolved_qname(name_table, refer_ns, refer.local_name)
                                })
                            })
                            .unwrap_or_else(|| "<unknown>".to_string());
                        vec![errors::error(
                            "cvc-identity-constraint.4.3",
                            format!(
                                "Keyref '{}' references unknown constraint '{}'",
                                keyref_name, refer_display
                            ),
                            None,
                        )]
                    }
                    Some(rk) => {
                        let target = self
                            .ic_scope_tables
                            .last()
                            .and_then(|slot| slot.as_ref())
                            .and_then(|map| map.get(&rk));
                        match target {
                            Some(target_table) => {
                                target_table.check_keyref(&keyref_table, name_table)
                            }
                            // No node table for the referenced key in E's
                            // table: every complete keyref member is unmatched.
                            None => {
                                let target_name =
                                    self.schema_set.arenas.identity_constraints[rk].name;
                                keyref_table.check_keyref_against(
                                    &KeyTable::new(rk, target_name, IdentityKind::Key),
                                    name_table,
                                )
                            }
                        }
                    }
                };
                if !errs.is_empty() {
                    invalidate(validity);
                }
                for err in errs {
                    if !self.continue_run() {
                        return;
                    }
                    self.emit_error_to(err, error_codes);
                }
            }
        }
    }

    /// An identity-constraint violation makes the constraint's owner element
    /// invalid (§3.3.4.3 clause 6). `closing` is the `[validity]` of the
    /// element being closed, already popped off the stack: it is the owner
    /// when `owner_depth` equals the stack length.
    fn invalidate_ic_owner(&mut self, owner_depth: usize, closing: &mut SchemaValidity) {
        if owner_depth == self.validation_stack.len() {
            invalidate(closing);
        } else {
            self.invalidate_open_ic_owner(owner_depth);
        }
    }

    /// [`invalidate_ic_owner`](Self::invalidate_ic_owner) for a violation
    /// found while the owner is still open (on the validation stack).
    fn invalidate_open_ic_owner(&mut self, owner_depth: usize) {
        if let Some(owner) = self.validation_stack.get_mut(owner_depth) {
            invalidate(&mut owner.validity);
        }
    }

    /// Check if an ENTITY/ENTITIES value names declared unparsed entities.
    ///
    /// §3.16.4 String Valid clause 3: "Every ENTITY value in V is a declared
    /// entity name." Only checked when `unparsed_entities` is set. Returns
    /// `true` when an undeclared name was reported: the value is then not
    /// String Valid, and the caller invalidates the item that carries it.
    fn check_entity_declared(&mut self, typed_value: &XmlValue) -> bool {
        use crate::types::value::XmlValueKind;
        let entities = match &self.unparsed_entities {
            Some(e) => e,
            None => return false,
        };
        // Collect undeclared names first to avoid borrow conflict with report_error
        let mut undeclared: Vec<String> = Vec::new();

        // A union value's ENTITY values are those of the member type that
        // validated it (a list member type included, `xs:ENTITIES` say), so
        // look through the union wrappers first — as `collect_id_idref` does.
        let mut typed_value = typed_value;
        while let XmlValueKind::Union(inner) = &typed_value.value {
            typed_value = inner.as_ref();
        }

        // Check list values first — built-in xs:ENTITIES, a custom
        // <xs:list itemType="xs:ENTITY">, and a list of a union with an
        // xs:ENTITY member: every member carries the type that validated it,
        // so exactly the ENTITY members are names to check.
        if let XmlValueKind::List { items, .. } = &typed_value.value {
            for item in items {
                if item.type_code == XmlTypeCode::Entity {
                    let name = item.to_string_value();
                    if !entities.contains(&name) {
                        undeclared.push(name);
                    }
                }
            }
        } else if typed_value.type_code == XmlTypeCode::Entity {
            let name = typed_value.to_string_value();
            if !entities.contains(&name) {
                undeclared.push(name);
            }
        }

        let any_undeclared = !undeclared.is_empty();
        for name in undeclared {
            self.report_error(
                "cvc-datatype-valid.1.2.1",
                format!("ENTITY '{}' is not declared as an unparsed entity", name),
            );
        }
        any_undeclared
    }

    /// Register an ID value with its owner element serial.
    ///
    /// XSD 1.1 §3.17.5.2: the [binding] is a set of elements. Same ID on the
    /// same owner element is allowed (set size stays 1); same ID on different
    /// elements is cvc-id.2.
    fn register_id_value(&mut self, value: String, owner_serial: u64) {
        match self.id_values.get(&value) {
            Some(&existing_serial) => {
                if !(self.schema_set.is_xsd11() && existing_serial == owner_serial) {
                    self.report_error("cvc-id.2", format!("Duplicate ID value '{}'", value));
                    // A verdict on the validation root, applied when it closes.
                    self.root_id_conflict = true;
                }
            }
            None => {
                self.id_values.insert(value, owner_serial);
            }
        }
    }

    /// Detect ID/IDREF types and collect values for finalization.
    ///
    /// Uses the normalized value from `typed_value` (not raw `value_str`)
    /// for ID and IDREF tracking, so whitespace-collapsed values match
    /// consistently across ID, IDREF, and IDREFS.
    ///
    /// `owner_serial` identifies the element that owns the ID binding per
    /// §3.17.5.2: for attributes it is the element carrying the attribute;
    /// for element text content it is the parent element.
    ///
    /// Handles both built-in types and user-defined list/union types
    /// containing ID/IDREF (§3.17.5.2: eligible items include types
    /// "derived or constructed directly or indirectly from" ID/IDREF).
    fn collect_id_idref(&mut self, typed_value: &XmlValue, value_str: &str, owner_serial: u64) {
        use crate::types::value::XmlValueKind;

        // Unwrap union recursively to get the effective value
        let mut effective = typed_value;
        while let XmlValueKind::Union(inner) = &effective.value {
            effective = inner.as_ref();
        }

        // List values — built-in xs:IDREFS, custom lists of xs:ID / xs:IDREF,
        // and lists of a union with an ID / IDREF member type: every member
        // carries the type that validated it (§3.17.5.2: eligible items include
        // types "derived or constructed directly or indirectly from" ID/IDREF),
        // so each member is dispatched on its own type.
        if let XmlValueKind::List { items, .. } = &effective.value {
            for item in items {
                match item.type_code {
                    XmlTypeCode::Id => {
                        self.register_id_value(item.to_string_value(), owner_serial);
                    }
                    XmlTypeCode::IdRef => {
                        let context = self.capture_context();
                        self.pending_idrefs.push((
                            item.to_string_value(),
                            self.current_location.clone(),
                            context,
                        ));
                    }
                    _ => {} // e.g. integer — not ID/IDREF
                }
            }
            return;
        }

        // Fall back to type_code-based dispatch for atomic values and
        // built-in list types (IdRefs, Entities).
        match effective.type_code {
            XmlTypeCode::Id => {
                let normalized = effective.to_string_value();
                self.register_id_value(normalized, owner_serial);
            }
            XmlTypeCode::IdRef | XmlTypeCode::IdRefs => {
                if effective.type_code == XmlTypeCode::IdRefs {
                    // Fallback for IdRefs without parsed list: split lexical text
                    for token in value_str.split_whitespace() {
                        let context = self.capture_context();
                        self.pending_idrefs.push((
                            token.to_string(),
                            self.current_location.clone(),
                            context,
                        ));
                    }
                } else {
                    // Single IdRef value
                    let context = self.capture_context();
                    self.pending_idrefs.push((
                        effective.to_string_value(),
                        self.current_location.clone(),
                        context,
                    ));
                }
            }
            _ => {}
        }
    }

    /// Validate a single attribute against a complex type's attribute
    /// declarations, wildcards, and fixed-value constraints. Shared by
    /// `validate_attribute` and `validate_deferred_attributes`.
    fn validate_attribute_against_type(
        &mut self,
        ct_key: ComplexTypeKey,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        let mut wildcard_cache = None;
        let found =
            self.resolve_attribute_lookup(ct_key, local_name, namespace, &mut wildcard_cache);

        match found {
            AttributeLookup::Found(attr_key, attr_type, fixed_value, fixed_source, inheritable) => {
                self.validate_declared_attribute(
                    attr_key,
                    attr_type,
                    fixed_value,
                    fixed_source,
                    inheritable,
                    local_name,
                    namespace,
                    value,
                )
            }
            AttributeLookup::Prohibited => {
                let attr_name = self.schema_set.name_table.resolve(local_name);
                self.report_native_error(
                    "cvc-complex-type.3.2.2",
                    format!("Attribute '{}' is prohibited", attr_name),
                    errors::NativeCause::ProhibitedAttribute,
                );
                self.mark_current_invalid();
                SchemaInfo::invalid()
            }
            AttributeLookup::NotFound => self.validate_attribute_via_wildcard(
                ct_key,
                local_name,
                namespace,
                value,
                wildcard_cache,
            ),
        }
    }

    /// Look the attribute up in the complex type's attribute uses, applying the
    /// cvc-complex-type.3.2.2 prohibited-declaration rescue.
    ///
    /// Returns the resolved lookup together with the effective attribute
    /// wildcard when the rescue already had to compute it, so the wildcard path
    /// does not recompute it.
    fn resolve_attribute_lookup(
        &self,
        ct_key: ComplexTypeKey,
        local_name: NameId,
        namespace: Option<NameId>,
        wildcard_cache: &mut Option<Option<crate::schema::derivation::EffectiveAttributeWildcard>>,
    ) -> AttributeLookup {
        let found = {
            let ct_data = &self.schema_set.arenas.complex_types[ct_key];
            self.find_attribute_in_type(ct_data, local_name, namespace)
        };
        // cvc-complex-type.3.2.2: clause 3.2.2 (wildcard) is checked
        // independently of 3.2.1 — a Prohibited declaration does not block a
        // matching wildcard (XSD 1.0 behaviour; W3C attZ002, addB034, addB136).
        // Applies identically under XSD 1.1: §3.2.2 mapping drops
        // use="prohibited" from {attribute uses}, so §3.4.4.2 clause 2.1
        // never matches and the fall-through to clause 2.2 (wildcard) is
        // the spec-compliant rescue. The §3.4.4.2 clause-4 Note about
        // "attribute use always takes precedence" is non-normative and
        // addresses only non-prohibited matches.
        //
        // The wildcard is the FULL effective attribute wildcard per
        // §3.6.2.2 (own + groups intersection) chained with §3.4.2.5's
        // extension-union over the base chain — `compute_runtime_attribute_wildcard`
        // returns this canonical form. Cached so the rescued-prohibited
        // path does not recompute when falling through to NotFound.
        match found {
            AttributeLookup::Prohibited => {
                let wc = crate::schema::derivation::compute_runtime_attribute_wildcard(
                    self.schema_set,
                    ct_key,
                );
                let rescued = match wc.as_ref() {
                    Some(w) => crate::schema::derivation::effective_wildcard_allows_attribute(
                        self.schema_set,
                        w,
                        namespace,
                        local_name,
                    ),
                    None => false,
                };
                *wildcard_cache = Some(wc);
                if rescued {
                    AttributeLookup::NotFound
                } else {
                    AttributeLookup::Prohibited
                }
            }
            other => other,
        }
    }

    /// Validate an attribute's value against its type — an `xs:QName` /
    /// `xs:NOTATION` value with the owner element's in-scope namespaces
    /// (Datatypes §3.3.18: "the bindings to be used in the ·lexical mapping·
    /// are those in the [in-scope namespaces] property of the relevant
    /// element").
    fn validate_attribute_value(
        &self,
        value: &str,
        type_key: TypeKey,
    ) -> Result<super::simple::SimpleTypeResult, ValidationError> {
        let names = &self.schema_set.name_table;
        let resolver = self
            .validation_stack
            .last()
            .and_then(|ev| ev.ns_context.as_ref())
            .map(|ns| super::simple::snapshot_prefixes(ns, names));
        super::simple::validate_simple_type_in(
            value,
            type_key,
            self.schema_set,
            resolver
                .as_ref()
                .map(|r| r as super::simple::PrefixResolver<'_>),
        )
    }

    /// Validate an attribute against the attribute declaration that matched it
    /// (§3.4.4.2 clause 2.1): value validation, the QName/NOTATION prefix
    /// binding check, the fixed-value constraint (cvc-attribute.4), and the
    /// XSD 1.1 `{inheritable}` record.
    #[allow(clippy::too_many_arguments)]
    fn validate_declared_attribute(
        &mut self,
        attr_key: Option<AttributeKey>,
        attr_type: Option<TypeKey>,
        fixed_value: Option<String>,
        fixed_source: Option<SourceRef>,
        inheritable: bool,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        // Parse value once; reused for the fixed-value check and for SchemaInfo.
        let mut member_type = None;
        let mut typed_value = None;
        let mut normalized_value = None;
        let mut attr_validity = SchemaValidity::Valid;
        // PERF_LAZY_PSVI Phase 2: skip materialization when opted out and
        // no consumer needs the value (no fixed, no active IC; the fast
        // path itself excludes ID/QName/value-space types).
        let mut used_fast_path = false;
        if let Some(type_key) = attr_type {
            let try_fast = self.psvi_retention_disabled() && fixed_value.is_none();
            if try_fast {
                if let Some(res) =
                    super::simple::try_validate_lexical_only(value, type_key, self.schema_set)
                {
                    used_fast_path = true;
                    if let Err(err) = res {
                        let err = err
                            .with_type_context(type_key, self.schema_set)
                            .with_component(attr_key.and_then(|k| {
                                errors::SchemaComponent::for_attribute(k, self.schema_set)
                            }));
                        self.report_validation_error(err);
                        attr_validity = SchemaValidity::Invalid;
                        self.mark_current_invalid();
                    }
                }
            }
            if !used_fast_path {
                let (outcome, raised) = self
                    .collect_expression_warnings(|| self.validate_attribute_value(value, type_key));
                match outcome {
                    Ok(result) => {
                        self.note_decimal_capacity(&result.typed_value);
                        self.note_seconds_capacity(&result.typed_value, value);
                        self.report_expression_warnings(raised);
                        member_type = result.member_type;
                        typed_value = Some(result.typed_value);
                        normalized_value = result.normalized_value;
                    }
                    Err(err) => {
                        self.report_expression_warnings(raised);
                        let err = err
                            .with_type_context(type_key, self.schema_set)
                            .with_component(attr_key.and_then(|k| {
                                errors::SchemaComponent::for_attribute(k, self.schema_set)
                            }));
                        self.report_validation_error(err);
                        attr_validity = SchemaValidity::Invalid;
                        self.mark_current_invalid();
                    }
                }
            }
        }

        // QName/NOTATION prefix-binding check (Datatypes §3.3.18 /
        // §3.3.19) — the simple-type validator can't see namespaces.
        // Skipped on the fast path (string-family types are never QName).
        if !used_fast_path && attr_validity == SchemaValidity::Valid {
            self.check_attribute_qname_prefixes(
                &mut typed_value,
                &mut attr_validity,
                local_name,
                value,
            );
        }

        if let Some(fixed) = fixed_value {
            let matches = if let Some(ref tv) = typed_value {
                super::simple::fixed_matches_typed_in(
                    value,
                    tv,
                    &fixed,
                    self.schema_set
                        .value_constraint_namespaces(fixed_source.as_ref()),
                    attr_type,
                    self.schema_set,
                )
            } else {
                super::simple::fixed_values_equal(value, &fixed, attr_type, self.schema_set)
            };
            if !matches {
                let attr_name = self.schema_set.name_table.resolve(local_name);
                let err = errors::error(
                    "cvc-attribute.4",
                    format!(
                        "Attribute '{}' has fixed value '{}' but got '{}'",
                        attr_name, fixed, value
                    ),
                    self.current_location.clone(),
                )
                .with_native_cause(errors::NativeCause::AttributeFixedValue {
                    expected: fixed,
                    actual: value.to_owned(),
                })
                .with_component(
                    attr_key
                        .and_then(|k| errors::SchemaComponent::for_attribute(k, self.schema_set)),
                );
                let err = match attr_type {
                    Some(key) => err.with_type_context(key, self.schema_set),
                    None => err,
                };
                self.report_validation_error(err);
                self.mark_current_invalid();
            }
        }

        // Record inheritable attribute into outgoing map for descendants
        // (XSD 1.1 §3.3.5.6). This shadows any ancestor value with the
        // same expanded name per the nearest-owner rule.
        #[cfg(feature = "xsd11")]
        if inheritable {
            if let Some(ev) = self.validation_stack.last_mut() {
                use super::context::InheritedAttributeValue;
                ev.outgoing_inherited.insert(
                    (namespace, local_name),
                    InheritedAttributeValue {
                        value: value.to_string(),
                        attribute_key: attr_key,
                    },
                );
            }
        }
        let _ = inheritable;

        let result = SchemaInfo {
            element_decl: None,
            attribute_decl: attr_key,
            schema_type: attr_type,
            member_type,
            validity: attr_validity,
            validation_attempted: ValidationAttempted::Full,
            is_default: false,
            is_nil: false,
            content_type: None,
            typed_value,
            normalized_value,
            schema_error_codes: Vec::new(),
            notation: None,
            deferred_by_cta: false,
            type_source: Some(TypeSource::Declaration),
            #[cfg(feature = "xsd11")]
            cta_selected: false,
            #[cfg(feature = "xsd11")]
            assertion_outcome: None,
        };
        self.post_process_attribute(local_name, namespace, value, &result);
        result
    }

    /// Validate an attribute that no attribute use matched against the type's
    /// effective attribute wildcard (§3.4.4.2 clause 2.2). An attribute the
    /// wildcard does not allow is a cvc-complex-type.3.2.2 violation.
    fn validate_attribute_via_wildcard(
        &mut self,
        ct_key: ComplexTypeKey,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
        wildcard_cache: Option<Option<crate::schema::derivation::EffectiveAttributeWildcard>>,
    ) -> SchemaInfo {
        let effective_wildcard = wildcard_cache.unwrap_or_else(|| {
            crate::schema::derivation::compute_runtime_attribute_wildcard(self.schema_set, ct_key)
        });
        if let Some(ref wildcard) = effective_wildcard {
            if crate::schema::derivation::effective_wildcard_allows_attribute(
                self.schema_set,
                wildcard,
                namespace,
                local_name,
            ) {
                let result = match wildcard.process_contents {
                    ProcessContents::Skip => SchemaInfo::empty(),
                    ProcessContents::Strict => {
                        self.validate_wildcard_attribute_strict(local_name, namespace, value)
                    }
                    ProcessContents::Lax => {
                        self.validate_wildcard_attribute_lax(local_name, namespace, value)
                    }
                };
                // Wildcard-backed inheritance (XSD 1.1 §3.3.5.6 clause 3.2):
                // If strict/lax resolved a governing declaration with
                // {inheritable}=true, record for propagation. Skip has no
                // governing declaration (attribute_decl is None).
                #[cfg(feature = "xsd11")]
                if let Some(attr_key) = result.attribute_decl {
                    if let Some(decl) = self.schema_set.arenas.attributes.get(attr_key) {
                        if decl.inheritable {
                            if let Some(ev) = self.validation_stack.last_mut() {
                                use super::context::InheritedAttributeValue;
                                ev.outgoing_inherited.insert(
                                    (namespace, local_name),
                                    InheritedAttributeValue {
                                        value: value.to_string(),
                                        attribute_key: Some(attr_key),
                                    },
                                );
                            }
                        }
                    }
                }
                if wildcard.process_contents == ProcessContents::Skip {
                    self.count_skipped_attribute_ic_matches(local_name, namespace);
                } else {
                    self.post_process_attribute(local_name, namespace, value, &result);
                }
                return result;
            }
        }

        let attr_name = self.schema_set.name_table.resolve(local_name);
        self.report_native_error(
            "cvc-complex-type.3.2.2",
            format!("Attribute '{}' is not allowed for this element", attr_name),
            errors::NativeCause::AttributeNotAllowed,
        );
        self.mark_current_invalid();
        SchemaInfo::invalid()
    }

    /// Count a skip-processed attribute toward identity-constraint field
    /// multi-node detection (cvc-identity-constraint.4.2.1).
    ///
    /// Skip-processed attributes cannot contribute a typed value to an IC field
    /// (§3.11.4), so the field slot stays absent — `xs:key` will report a
    /// missing-field violation. However, the attribute IS still selected by the
    /// field XPath, so it must count toward multi-node detection.
    fn count_skipped_attribute_ic_matches(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
    ) {
        let ns = namespace.unwrap_or(NameId(0));
        let mut multi_node_ic: Vec<(NameId, usize, usize)> = Vec::new();
        for cs in &mut self.active_constraints {
            let matches = cs.matching_fields(local_name, ns);
            for field_idx in matches {
                if cs.increment_field_match_count(field_idx) {
                    multi_node_ic.push((
                        cs.key_table.table().constraint_name,
                        field_idx,
                        cs.owner_depth,
                    ));
                }
            }
        }
        for (constraint_name, field_idx, owner_depth) in multi_node_ic {
            let name = self
                .schema_set
                .name_table
                .resolve(constraint_name)
                .to_string();
            self.report_error(
                "cvc-identity-constraint.4.2.1",
                format!(
                    "Identity constraint '{}': field {} matches more than one node",
                    name,
                    field_idx + 1
                ),
            );
            self.invalidate_open_ic_owner(owner_depth);
        }
    }

    /// Re-validate attributes that were deferred during CTA evaluation.
    ///
    /// Called from `validate_end_of_attributes()` after the type alternative
    /// has been selected.
    #[cfg(feature = "xsd11")]
    fn validate_deferred_attributes(&mut self, schema_type: Option<TypeKey>) {
        self.deferred_attribute_results.clear();

        let collected = match self.validation_stack.last_mut() {
            Some(ev) => std::mem::take(&mut ev.collected_attributes),
            None => return,
        };
        // The attributes the caller asked to skip: skipped now unless an
        // attribute use of the selected type matches them (§3.4.4.2 clause
        // 2.1), in which case they are validated like the others.
        let skip_requested = std::mem::take(&mut self.skipped_collected);
        let process_contents = self
            .validation_stack
            .last()
            .map_or(ContentProcessing::Strict, |ev| ev.process_contents);

        let ct_key = match schema_type {
            Some(TypeKey::Complex(k)) => k,
            _ => {
                // Non-complex type (e.g. simple type selected by CTA):
                // no attribute declarations exist, but we must produce one
                // result per collected attribute to keep the 1:1 invariant
                // with the typed builder's deferred_attr_refs.
                for &i in &skip_requested {
                    if let Some((namespace, local_name, _)) = collected.get(i) {
                        self.current_attribute = Some((*namespace, *local_name));
                        self.record_skipped_attribute(*local_name, *namespace);
                        self.current_attribute = None;
                    }
                }
                self.deferred_attribute_results
                    .resize_with(collected.len(), SchemaInfo::empty);
                return;
            }
        };

        for (i, (namespace, local_name, value)) in collected.iter().enumerate() {
            if skip_requested.contains(&i)
                && !self.attribute_use_matches(
                    schema_type,
                    process_contents,
                    *local_name,
                    *namespace,
                )
            {
                self.current_attribute = Some((*namespace, *local_name));
                self.record_skipped_attribute(*local_name, *namespace);
                self.current_attribute = None;
                self.deferred_attribute_results.push(SchemaInfo::empty());
                continue;
            }
            let ec_snapshot = self
                .validation_stack
                .last()
                .map_or(0, |ev| ev.error_codes.len());
            self.current_attribute = Some((*namespace, *local_name));
            let mut info =
                self.validate_attribute_against_type(ct_key, *local_name, *namespace, value);
            self.current_attribute = None;
            self.fold_attribute_validity(&info);
            if let Some(ev) = self.validation_stack.last_mut() {
                if ev.error_codes.len() > ec_snapshot {
                    info.schema_error_codes = ev.error_codes[ec_snapshot..].to_vec();
                    ev.error_codes.truncate(ec_snapshot);
                }
                // Track attribute [validation attempted] on parent element
                match info.validation_attempted {
                    ValidationAttempted::Full => {
                        ev.any_attr_not_none = true;
                    }
                    ValidationAttempted::None => {
                        ev.any_attr_not_full = true;
                    }
                    ValidationAttempted::Partial => {
                        ev.any_attr_not_full = true;
                        ev.any_attr_not_none = true;
                    }
                }
            }
            self.deferred_attribute_results.push(info);
        }
    }

    /// Drain deferred attribute validation results collected during CTA processing.
    ///
    /// Returns the `SchemaInfo` results in the same order as the attributes were
    /// originally encountered. The internal buffer is emptied.
    #[cfg(feature = "xsd11")]
    pub fn take_deferred_attribute_results(&mut self) -> Vec<SchemaInfo> {
        std::mem::take(&mut self.deferred_attribute_results)
    }

    /// Post-process a validated attribute for identity constraint field matching
    /// and ID/IDREF collection.
    fn post_process_attribute(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
        result: &SchemaInfo,
    ) {
        let ns = namespace.unwrap_or(NameId(0));

        // Identity constraint: check field attribute matches. The whole block
        // — including the QName re-resolution and the typed-value clone it
        // needs — is gated on an active constraint (P0 allocation gate,
        // XSD_COMPILER_REWORK §12.1). The ID/IDREF/ENTITY/NOTATION checks
        // below are independent and always run.
        if !self.active_constraints.is_empty() {
            let ns_ctx = self
                .validation_stack
                .last()
                .and_then(|ev| ev.ns_context.as_ref());
            let ic_typed_value = Self::resolve_ic_qname_value(
                &result.typed_value,
                value,
                ns_ctx,
                &self.schema_set.name_table,
            )
            .or_else(|| result.typed_value.clone());
            let mut multi_node_ic: Vec<(NameId, usize, usize)> = Vec::new();
            for cs in &mut self.active_constraints {
                let matches = cs.matching_fields(local_name, ns);
                for field_idx in matches {
                    let already_matched =
                        cs.set_field_value(field_idx, value.to_string(), ic_typed_value.clone());
                    if already_matched {
                        multi_node_ic.push((
                            cs.key_table.table().constraint_name,
                            field_idx,
                            cs.owner_depth,
                        ));
                    }
                }
            }
            for (constraint_name, field_idx, owner_depth) in multi_node_ic {
                let name = self
                    .schema_set
                    .name_table
                    .resolve(constraint_name)
                    .to_string();
                self.report_error(
                    "cvc-identity-constraint.4.2.1",
                    format!(
                        "Identity constraint '{}': field {} matches more than one node",
                        name,
                        field_idx + 1
                    ),
                );
                self.invalidate_open_ic_owner(owner_depth);
            }
        }

        // ID/IDREF/ENTITY collection — owner is current element (attribute binding)
        if let Some(ref tv) = result.typed_value {
            let owner = self
                .validation_stack
                .last()
                .map(|e| e.element_serial)
                .unwrap_or(0);
            self.collect_id_idref(tv, value, owner);
            if self.check_entity_declared(tv) {
                self.invalidate_attribute_owner();
            }

            // NOTATION tracking (§3.14.5): set [notation] on parent element
            if tv.type_code == XmlTypeCode::Notation {
                // Resolve before borrowing the stack to avoid borrow conflict
                let notation = self
                    .validation_stack
                    .last()
                    .filter(|ev| ev.notation.is_none())
                    .and_then(|ev| ev.ns_context.as_ref())
                    .and_then(|ctx| self.resolve_notation_qname(value, ctx));
                if let (Some(nk), Some(ev)) = (notation, self.validation_stack.last_mut()) {
                    if ev.notation.is_none() {
                        ev.notation = Some(nk);
                    }
                }
            }
        }
    }

    /// Resolve a NOTATION QName value to a NotationKey using the element's
    /// namespace context. Returns `None` if the QName is malformed or the
    /// notation is not declared.
    fn resolve_notation_qname(
        &self,
        value: &str,
        ns_context: &NamespaceContextSnapshot,
    ) -> Option<NotationKey> {
        let qn =
            parse_qname_with_snapshot(value, ns_context, &self.schema_set.name_table, true).ok()?;
        self.schema_set
            .lookup_notation(qn.namespace_uri, qn.local_name)
    }

    /// The QName / NOTATION prefix-binding check of an attribute's value
    /// (Datatypes §3.3.18: "the bindings to be used in the ·lexical mapping·
    /// are those in the [in-scope namespaces] property of the relevant
    /// element") — a declared attribute and one a `strict` or `lax` wildcard
    /// admits alike, of an atomic, list or union type: a prefix bound nowhere
    /// is `cvc-datatype-valid.1.2.1`, the attribute and its element are
    /// invalid; a value resolved here replaces `typed_value`.
    fn check_attribute_qname_prefixes(
        &mut self,
        typed_value: &mut Option<XmlValue>,
        attr_validity: &mut SchemaValidity,
        local_name: NameId,
        value: &str,
    ) {
        match self.resolve_attribute_qname_typed_value(typed_value.as_ref(), value) {
            Ok(Some(resolved)) => *typed_value = Some(resolved),
            Ok(None) => {}
            Err(prefix) => {
                let attr_name = self.schema_set.name_table.resolve(local_name);
                self.report_error(
                    "cvc-datatype-valid.1.2.1",
                    format!(
                        "Attribute '{}' has QName value '{}' with undeclared prefix '{}'",
                        attr_name, value, prefix
                    ),
                );
                self.mark_current_invalid();
                *attr_validity = SchemaValidity::Invalid;
            }
        }
    }

    /// Resolve a QName/NOTATION attribute's typed value against the current
    /// element's namespace context.
    ///
    /// Returns:
    /// - `Ok(Some(value))` — typed value has been re-built with a resolved
    ///   `XmlAtomicValue::QName` / `Notation`; caller should replace.
    /// - `Ok(None)` — nothing to do (non-QName type, no ns_context, or
    ///   non-`UndefinedPrefix` lexical error already flagged by the simple
    ///   type validator).
    /// - `Err(prefix)` — the attribute carries a prefix that is not in
    ///   scope; caller should report cvc-datatype-valid.1.2.1.
    fn resolve_attribute_qname_typed_value(
        &self,
        typed_value: Option<&XmlValue>,
        value: &str,
    ) -> Result<Option<XmlValue>, String> {
        let ctx = self
            .validation_stack
            .last()
            .and_then(|ev| ev.ns_context.as_ref());
        self.resolve_qname_typed_value_in(typed_value, value, ctx)
    }

    /// [`resolve_attribute_qname_typed_value`](Self::resolve_attribute_qname_typed_value)
    /// with the namespace bindings `ctx` — the instance element's, or, for a
    /// value constraint, those in scope at the declaration that carries it.
    fn resolve_qname_typed_value_in(
        &self,
        typed_value: Option<&XmlValue>,
        value: &str,
        ctx: Option<&NamespaceContextSnapshot>,
    ) -> Result<Option<XmlValue>, String> {
        use crate::types::value::{QNameValue, XmlAtomicValue, XmlValueKind};
        let Some(tv) = typed_value else {
            return Ok(None);
        };
        // Validation resolved it already with these bindings: only a value
        // whose prefix they bind nowhere is still in lexical form.
        if !super::simple::holds_lexical_qname(tv) {
            return Ok(None);
        }
        let is_list = super::simple::holds_list(tv);
        // A list carries the code of its item type, which for a list of a
        // union is `xs:anySimpleType` whatever its members are: a list is
        // decided by its members' own codes below.
        if !is_list && tv.type_code != XmlTypeCode::QName && tv.type_code != XmlTypeCode::Notation {
            return Ok(None);
        }
        let Some(ctx) = ctx else {
            return Ok(None);
        };
        // A list value (also the one a union's chosen member type gave):
        // every QName / NOTATION member on its own.
        if is_list {
            return resolve_list_qname_members(tv, ctx, &self.schema_set.name_table);
        }
        let qn = match parse_qname_with_snapshot(value, ctx, &self.schema_set.name_table, true) {
            Ok(qn) => qn,
            Err(QNameError::UndefinedPrefix(prefix)) => return Err(prefix),
            Err(_) => return Ok(None),
        };
        let qn = QNameValue::new(qn, &self.schema_set.name_table);
        let atom = if tv.type_code == XmlTypeCode::Notation {
            XmlAtomicValue::Notation(qn)
        } else {
            XmlAtomicValue::QName(qn)
        };
        Ok(Some(XmlValue {
            type_code: tv.type_code,
            schema_type: tv.schema_type,
            value: XmlValueKind::Atomic(atom),
        }))
    }

    /// Resolve a QName/NOTATION-typed value for IC purposes.
    ///
    /// QName values from simple type validation are stored as `XmlAtomicValue::String`
    /// because namespace context wasn't available at validation time. For IC field
    /// value comparison, we need namespace-aware QNames. Returns a new resolved
    /// copy (does NOT mutate the original, preserving PSVI integrity).
    fn resolve_ic_qname_value(
        typed_value: &Option<XmlValue>,
        string_value: &str,
        ns_context: Option<&NamespaceContextSnapshot>,
        name_table: &crate::namespace::table::NameTable,
    ) -> Option<XmlValue> {
        use crate::types::value::{QNameValue, XmlAtomicValue, XmlValueKind};
        let val = typed_value.as_ref()?;
        if val.type_code != XmlTypeCode::QName && val.type_code != XmlTypeCode::Notation {
            return None;
        }
        // Attribute validation may have already resolved the QName/NOTATION
        // against the element's namespace context (see
        // `validate_attribute_against_type`). Skip the re-parse in that case.
        if matches!(
            val.value,
            XmlValueKind::Atomic(XmlAtomicValue::QName(_))
                | XmlValueKind::Atomic(XmlAtomicValue::Notation(_))
        ) {
            return None;
        }
        let ctx = ns_context?;
        let qn = parse_qname_with_snapshot(string_value, ctx, name_table, true).ok()?;
        let qn = QNameValue::new(qn, name_table);
        let atom = if val.type_code == XmlTypeCode::Notation {
            XmlAtomicValue::Notation(qn)
        } else {
            XmlAtomicValue::QName(qn)
        };
        Some(XmlValue {
            type_code: val.type_code,
            schema_type: val.schema_type,
            value: XmlValueKind::Atomic(atom),
        })
    }

    /// Initialize content model and ContentType from a TypeKey
    fn init_content_model(
        &mut self,
        type_key: Option<TypeKey>,
    ) -> (ContentValidatorState, ContentType) {
        match type_key {
            Some(TypeKey::Complex(ct_key)) => {
                let schema_set = self.schema_set;
                let ct_data = &schema_set.arenas.complex_types[ct_key];
                let content_type = determine_content_type(schema_set, ct_data);

                let content_state = match content_type {
                    ContentType::Empty => ContentValidatorState::Empty,
                    ContentType::TextOnly => ContentValidatorState::Simple,
                    ContentType::ElementOnly | ContentType::Mixed => {
                        // The content model is identical for every instance of
                        // this complex type and was compiled once, at
                        // `SchemaValidator` construction (`build_content_models`).
                        // Per element we only clone the shared `Arc`s and build
                        // fresh active states. A missing entry means preparation
                        // failed at load time: that is an operational failure
                        // (assessment aborted), never silently empty content.
                        match self.content_models.models.get(&ct_key) {
                            Some(cm) => ContentValidatorState::from_compiled(cm),
                            None => {
                                self.fail_content_model_unavailable(ct_key);
                                ContentValidatorState::Empty
                            }
                        }
                    }
                };

                (content_state, content_type)
            }
            Some(TypeKey::Simple(_)) => (ContentValidatorState::Simple, ContentType::TextOnly),
            None => (ContentValidatorState::Simple, ContentType::TextOnly),
        }
    }

    /// Resolve an xsi:type QName string to an [`XsiTypeOutcome`].
    ///
    /// Errors are collected in `deferred_errors` instead of being emitted
    /// immediately, so the caller can emit them after the child element is
    /// pushed onto the validation stack (ensuring correct PSVI attribution).
    fn resolve_xsi_type(
        &self,
        xsi_type_str: &str,
        declared_type: Option<TypeKey>,
        block: DerivationSet,
        ns_context: &NamespaceContextSnapshot,
        deferred_errors: &mut Vec<ValidationError>,
    ) -> XsiTypeOutcome {
        // Parse and validate the QName using shared parsing logic
        let qname = match parse_qname_with_snapshot(
            xsi_type_str,
            ns_context,
            &self.schema_set.name_table,
            true,
        ) {
            Ok(qn) => qn,
            Err(e) => {
                let msg = match e {
                    QNameError::UndefinedPrefix(p) => {
                        format!(
                            "Undeclared prefix '{}' in xsi:type value '{}'",
                            p, xsi_type_str
                        )
                    }
                    _ => format!("Invalid xsi:type value '{}': {}", xsi_type_str, e),
                };
                deferred_errors.push(errors::error("cvc-elt.4.1", msg, None).with_native_cause(
                    errors::NativeCause::XsiTypeUnresolved {
                        value: xsi_type_str.to_owned(),
                    },
                ));
                return XsiTypeOutcome::Unresolved;
            }
        };

        // Look up the type
        let resolved = self.lookup_xsi_type(qname.namespace_uri, qname.local_name);

        match resolved {
            Some(type_key) => {
                // Validate derivation: the xsi:type must derive from the declared type
                if let Some(declared) = declared_type {
                    // cvc-elt.4.2: basic derivation check (no block keywords)
                    if !self.schema_set.is_type_derived_from(
                        type_key,
                        declared,
                        DerivationSet::empty(),
                    ) {
                        deferred_errors.push(
                            errors::error(
                                "cvc-elt.4.2",
                                format!(
                                    "xsi:type '{}' does not derive from the declared type",
                                    xsi_type_str
                                ),
                                None,
                            )
                            .with_native_cause(errors::NativeCause::XsiTypeDerivation {
                                value: xsi_type_str.to_owned(),
                                blocked: false,
                            })
                            .with_xsi_types(
                                declared,
                                type_key,
                                self.schema_set,
                            ),
                        );
                        return XsiTypeOutcome::InvalidDerivation;
                    }
                    // cvc-elt.4.3: validly substitutable — combine the element's
                    // {disallowed substitutions} (block) with the declared type's
                    // {prohibited substitutions} into a single exclusion mask.
                    let mut combined_block = block;
                    if let TypeKey::Complex(declared_ct_key) = declared {
                        if let Some(declared_ct) =
                            self.schema_set.arenas.complex_types.get(declared_ct_key)
                        {
                            combined_block |= declared_ct.block.element_block_mask();
                        }
                    }
                    if !combined_block.is_empty()
                        && !self
                            .schema_set
                            .is_type_derived_from(type_key, declared, combined_block)
                    {
                        deferred_errors.push(
                            errors::error(
                                "cvc-elt.4.3",
                                format!(
                                "xsi:type '{}' is not validly substitutable for the declared type \
                                 (blocked by 'block' attribute)",
                                xsi_type_str
                            ),
                                None,
                            )
                            .with_native_cause(errors::NativeCause::XsiTypeDerivation {
                                value: xsi_type_str.to_owned(),
                                blocked: true,
                            })
                            .with_xsi_types(
                                declared,
                                type_key,
                                self.schema_set,
                            ),
                        );
                        return XsiTypeOutcome::InvalidDerivation;
                    }
                }
                XsiTypeOutcome::Applied(type_key)
            }
            None => {
                deferred_errors.push(
                    errors::error(
                        "cvc-elt.4.1",
                        format!(
                            "Type '{}' specified in xsi:type is not declared",
                            xsi_type_str
                        ),
                        None,
                    )
                    .with_native_cause(
                        errors::NativeCause::XsiTypeUnresolved {
                            value: xsi_type_str.to_owned(),
                        },
                    ),
                );
                XsiTypeOutcome::Unresolved
            }
        }
    }

    /// Emit deferred xsi:type errors (collected by [`resolve_xsi_type`]).
    fn emit_deferred_xsi_type_errors(&mut self, errors: Vec<ValidationError>) {
        for err in errors {
            if !self.continue_run() {
                return;
            }
            let constraint = err.constraint;
            if constraint == "cvc-elt.4.1" {
                self.xsi_type_unresolved_reported =
                    self.validation_stack.last().map(|ev| ev.element_serial);
            }
            self.report_validation_error(err);
        }
    }

    /// Whether the `xsi:type` value `value` ·resolves· to a type definition
    /// with the current element's in-scope namespaces (§3.3.4.1 clause 3).
    fn xsi_type_resolves(&self, value: &str) -> bool {
        let empty = NamespaceContextSnapshot {
            default_ns: None,
            bindings: Vec::new(),
        };
        let ns_context = self
            .validation_stack
            .last()
            .and_then(|ev| ev.ns_context.as_ref())
            .unwrap_or(&empty);
        match parse_qname_with_snapshot(value, ns_context, &self.schema_set.name_table, true) {
            Ok(qname) => self
                .lookup_xsi_type(qname.namespace_uri, qname.local_name)
                .is_some(),
            Err(_) => false,
        }
    }

    /// The type definition an `xsi:type` naming `{namespace}local_name`
    /// ·resolves· to: a named type of the schema, or a built-in type the set's
    /// XSD version defines — and with [`ValidationFlags::XPATH_TYPES`] the
    /// types of the XPath 2.0 data model it does not
    /// ([`SchemaSet::xpath_type`]).
    fn lookup_xsi_type(&self, namespace: Option<NameId>, local_name: NameId) -> Option<TypeKey> {
        self.schema_set
            .lookup_type(namespace, local_name)
            .or_else(|| {
                self.schema_set.xsi_type_built_in(
                    namespace,
                    local_name,
                    self.flags.contains(ValidationFlags::XPATH_TYPES),
                )
            })
    }

    /// Returns xs:anyType's content model for lax assessment.
    ///
    /// The caller keeps `schema_type = None` (no governing type) — only the
    /// content model (Mixed + `(xs:any processContents=lax)*`) is used.
    fn lax_assessment_content_model(&mut self) -> (ContentValidatorState, ContentType) {
        let any_type_key = TypeKey::Complex(self.schema_set.any_type_key());
        self.init_content_model(Some(any_type_key))
    }

    /// Validate an attribute matched by a wildcard with processContents="strict".
    ///
    /// A global attribute declaration must exist; its value is validated against
    /// the declared type.
    fn validate_wildcard_attribute_strict(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        match self.schema_set.lookup_attribute(namespace, local_name) {
            Some(attr_key) => {
                let attr_data = self.schema_set.arenas.attributes.get(attr_key);
                let attr_type = attr_data.and_then(|d| d.resolved_type);
                let fixed = attr_data.and_then(|d| d.fixed_value.clone());
                let fixed_ns = self
                    .schema_set
                    .value_constraint_namespaces(attr_data.and_then(|d| d.source.as_ref()));

                // Parse value once; reused for the fixed-value check and SchemaInfo.
                let mut member_type = None;
                let mut typed_value = None;
                let mut normalized_value = None;
                let mut attr_validity = SchemaValidity::Valid;
                if let Some(type_key) = attr_type {
                    let (outcome, raised) = self.collect_expression_warnings(|| {
                        self.validate_attribute_value(value, type_key)
                    });
                    match outcome {
                        Ok(result) => {
                            self.note_decimal_capacity(&result.typed_value);
                            self.note_seconds_capacity(&result.typed_value, value);
                            self.report_expression_warnings(raised);
                            member_type = result.member_type;
                            typed_value = Some(result.typed_value);
                            normalized_value = result.normalized_value;
                        }
                        Err(err) => {
                            self.report_expression_warnings(raised);
                            self.report_validation_error(err);
                            attr_validity = SchemaValidity::Invalid;
                            if let Some(s) = self.validation_stack.last_mut() {
                                s.validity = SchemaValidity::Invalid;
                            }
                        }
                    }
                }
                if attr_validity == SchemaValidity::Valid {
                    self.check_attribute_qname_prefixes(
                        &mut typed_value,
                        &mut attr_validity,
                        local_name,
                        value,
                    );
                }

                if let Some(fixed_val) = fixed {
                    let matches = if let Some(ref tv) = typed_value {
                        super::simple::fixed_matches_typed_in(
                            value,
                            tv,
                            &fixed_val,
                            fixed_ns,
                            attr_type,
                            self.schema_set,
                        )
                    } else {
                        super::simple::fixed_values_equal(
                            value,
                            &fixed_val,
                            attr_type,
                            self.schema_set,
                        )
                    };
                    if !matches {
                        let attr_name = self.schema_set.name_table.resolve(local_name);
                        self.report_error(
                            "cvc-attribute.4",
                            format!(
                                "Attribute '{}' has fixed value '{}' but got '{}'",
                                attr_name, fixed_val, value
                            ),
                        );
                        if let Some(s) = self.validation_stack.last_mut() {
                            s.validity = SchemaValidity::Invalid;
                        }
                    }
                }

                SchemaInfo {
                    element_decl: None,
                    attribute_decl: Some(attr_key),
                    schema_type: attr_type,
                    member_type,
                    validity: attr_validity,
                    validation_attempted: ValidationAttempted::Full,
                    is_default: false,
                    is_nil: false,
                    content_type: None,
                    typed_value,
                    normalized_value,
                    schema_error_codes: Vec::new(),
                    notation: None,
                    deferred_by_cta: false,
                    type_source: Some(TypeSource::Declaration),
                    #[cfg(feature = "xsd11")]
                    cta_selected: false,
                    #[cfg(feature = "xsd11")]
                    assertion_outcome: None,
                }
            }
            None => {
                let attr_name = self.schema_set.name_table.resolve(local_name);
                self.report_error(
                    "cvc-assess-attr.1.2",
                    format!(
                        "No global attribute declaration for '{}' (wildcard processContents=\"strict\")",
                        attr_name
                    ),
                );
                if let Some(s) = self.validation_stack.last_mut() {
                    s.validity = SchemaValidity::Invalid;
                }
                SchemaInfo::invalid()
            }
        }
    }

    /// Validate an attribute matched by a wildcard with processContents="lax".
    ///
    /// If a global attribute declaration exists, validate; otherwise skip.
    fn validate_wildcard_attribute_lax(
        &mut self,
        local_name: NameId,
        namespace: Option<NameId>,
        value: &str,
    ) -> SchemaInfo {
        match self.schema_set.lookup_attribute(namespace, local_name) {
            Some(attr_key) => {
                // Found a global declaration — validate like strict
                let attr_data = self.schema_set.arenas.attributes.get(attr_key);
                let attr_type = attr_data.and_then(|d| d.resolved_type);
                let fixed = attr_data.and_then(|d| d.fixed_value.clone());
                let fixed_ns = self
                    .schema_set
                    .value_constraint_namespaces(attr_data.and_then(|d| d.source.as_ref()));

                // Parse value once; reused for the fixed-value check and SchemaInfo.
                let mut member_type = None;
                let mut typed_value = None;
                let mut normalized_value = None;
                let mut attr_validity = SchemaValidity::Valid;
                if let Some(type_key) = attr_type {
                    let (outcome, raised) = self.collect_expression_warnings(|| {
                        self.validate_attribute_value(value, type_key)
                    });
                    match outcome {
                        Ok(result) => {
                            self.note_decimal_capacity(&result.typed_value);
                            self.note_seconds_capacity(&result.typed_value, value);
                            self.report_expression_warnings(raised);
                            member_type = result.member_type;
                            typed_value = Some(result.typed_value);
                            normalized_value = result.normalized_value;
                        }
                        Err(err) => {
                            self.report_expression_warnings(raised);
                            self.report_validation_error(err);
                            attr_validity = SchemaValidity::Invalid;
                            if let Some(s) = self.validation_stack.last_mut() {
                                s.validity = SchemaValidity::Invalid;
                            }
                        }
                    }
                }
                if attr_validity == SchemaValidity::Valid {
                    self.check_attribute_qname_prefixes(
                        &mut typed_value,
                        &mut attr_validity,
                        local_name,
                        value,
                    );
                }

                if let Some(fixed_val) = fixed {
                    let matches = if let Some(ref tv) = typed_value {
                        super::simple::fixed_matches_typed_in(
                            value,
                            tv,
                            &fixed_val,
                            fixed_ns,
                            attr_type,
                            self.schema_set,
                        )
                    } else {
                        super::simple::fixed_values_equal(
                            value,
                            &fixed_val,
                            attr_type,
                            self.schema_set,
                        )
                    };
                    if !matches {
                        let attr_name = self.schema_set.name_table.resolve(local_name);
                        self.report_error(
                            "cvc-attribute.4",
                            format!(
                                "Attribute '{}' has fixed value '{}' but got '{}'",
                                attr_name, fixed_val, value
                            ),
                        );
                        if let Some(s) = self.validation_stack.last_mut() {
                            s.validity = SchemaValidity::Invalid;
                        }
                    }
                }

                SchemaInfo {
                    element_decl: None,
                    attribute_decl: Some(attr_key),
                    schema_type: attr_type,
                    member_type,
                    validity: attr_validity,
                    validation_attempted: ValidationAttempted::Full,
                    is_default: false,
                    is_nil: false,
                    content_type: None,
                    typed_value,
                    normalized_value,
                    schema_error_codes: Vec::new(),
                    notation: None,
                    deferred_by_cta: false,
                    type_source: Some(TypeSource::Declaration),
                    #[cfg(feature = "xsd11")]
                    cta_selected: false,
                    #[cfg(feature = "xsd11")]
                    assertion_outcome: None,
                }
            }
            None => {
                // No global declaration — lax means skip
                SchemaInfo::empty()
            }
        }
    }

    /// Collect all attribute uses from resolved attribute groups (recursively).
    fn collect_group_attributes(&self, ct_data: &ComplexTypeDefData) -> Vec<GroupAttribute> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        for &group_key in &ct_data.resolved_attribute_groups {
            self.collect_group_attributes_recursive(group_key, &mut result, &mut visited);
        }
        result
    }

    fn collect_group_attributes_recursive(
        &self,
        group_key: AttributeGroupKey,
        result: &mut Vec<GroupAttribute>,
        visited: &mut HashSet<AttributeGroupKey>,
    ) {
        if !visited.insert(group_key) {
            return; // prevent infinite recursion on circular refs
        }
        let group_data = match self.schema_set.arenas.get_attribute_group(group_key) {
            Some(g) => g,
            None => return,
        };
        for (i, attr_use) in group_data.attributes.iter().enumerate() {
            let resolved = group_data.resolved_attributes.get(i);
            let attr_key = resolved.and_then(|r| r.resolved_ref);
            // For `<xs:attribute ref="x:foo"/>` uses, the use's own
            // `resolved_type` is `None` — the type lives on the global
            // declaration. Fall back to the resolved type recorded on the
            // global decl, otherwise simple-type validation gets skipped
            // (the W3C `xsd003b` fixture exercises this through a redefined
            // `simpleType` whose enum-restricted facet must be applied to
            // an attribute reached via an attribute group).
            let attr_type = resolved.and_then(|r| r.resolved_type).or_else(|| {
                attr_key
                    .and_then(|k| self.schema_set.arenas.attributes.get(k))
                    .and_then(|d| d.resolved_type)
            });
            let (name, namespace) =
                self.resolve_attr_use_name_ns(attr_use, resolved, group_data.target_namespace);
            let (fixed_value, fixed_source) = self.attribute_use_fixed(attr_use, attr_key);
            let (value, value_source) = self.effective_value_constraint(attr_use, attr_key);
            result.push(GroupAttribute {
                name,
                namespace,
                use_kind: attr_use.use_kind,
                type_key: attr_type,
                attr_key,
                fixed_value,
                fixed_source,
                value: value.map(str::to_string),
                value_source: value_source.cloned(),
                #[cfg(feature = "xsd11")]
                inheritable: attr_use.use_inheritable(
                    attr_key.and_then(|k| self.schema_set.arenas.attributes.get(k)),
                ),
            });
        }
        for &nested_key in &group_data.resolved_attribute_groups {
            self.collect_group_attributes_recursive(nested_key, result, visited);
        }
    }

    /// Visit every member of the {attribute uses} of the complex type
    /// `ct_key`, in order: its own `<attribute>` uses, its attribute groups'
    /// uses, then those inherited from each base type down to `xs:anyType`.
    ///
    /// Structures §3.4.2.4 (Mapping Rule for Attribute Uses Property): "1 The
    /// set of attribute uses corresponding to the <attribute> \[children\], if
    /// any. 2 The {attribute uses} of the attribute groups … 3 The attribute
    /// uses "inherited" from the {base type definition} T" — for an extension
    /// (3.1) all of T's; for a restriction (3.2) all except those whose name is
    /// "3.2.1 the expanded name of the {attribute declaration} of an attribute
    /// use which has already been included in the set, following the rules in
    /// clause 1 or clause 2 above; 3.2.2 the expanded name of the {attribute
    /// declaration} of what would have been an attribute use corresponding to
    /// an <attribute> \[child\], if the <attribute> had not had use =
    /// prohibited." So a prohibition only stops a restriction's inheritance:
    /// it never hides a use of the type itself, an extension's is "simply
    /// ignored", and an attribute group's prohibited `<attribute>` is no use
    /// at all (XSD 1.0 §3.2.2: it "corresponds to nothing at all"; W3C
    /// Bugzilla #4043). The set is cumulative down the base chain, as the
    /// clause composes. A name is reported once.
    ///
    /// The one walk behind every runtime consumer of the attribute uses: the
    /// required-attribute check, the defaulted attributes (identity-constraint
    /// fields, ID/IDREF, `get_default_attributes`), the inheritable defaults
    /// and `get_expected_attributes`. [`find_attribute_in_type`](Self::find_attribute_in_type)
    /// looks one name up by the same rule.
    fn for_each_attribute_use(
        &self,
        ct_key: ComplexTypeKey,
        mut visit: impl FnMut(&AttributeUseEntry<'_>),
    ) {
        let any_type = self.schema_set.any_type_key();
        // Names a more derived restriction keeps out of what its base types
        // contribute (clauses 3.2.1 and 3.2.2).
        let mut not_inherited = AttrNames::new();
        let mut own = AttrNames::new();
        let mut prohibited = AttrNames::new();
        let mut current = Some(ct_key);
        while let Some(key) = current.filter(|&key| key != any_type) {
            let ct_data = &self.schema_set.arenas.complex_types[key];
            let base = match ct_data.resolved_base_type {
                Some(TypeKey::Complex(base)) if base != any_type => Some(base),
                _ => None,
            };
            let groups = self.collect_group_attributes(ct_data);
            // The names are needed only to keep a group use from repeating a
            // use of this type, and to stop inheritance from a base still to
            // come — not for the common type with neither.
            let track = base.is_some() || !groups.is_empty();
            own.clear();
            prohibited.clear();

            for (i, attr_use) in ct_data.attributes.iter().enumerate() {
                let resolved = ct_data.resolved_attributes.get(i);
                let (name, namespace) =
                    self.resolve_attr_use_name_ns(attr_use, resolved, ct_data.target_namespace);
                if attr_use.use_kind == AttributeUseKind::Prohibited {
                    if base.is_some() {
                        prohibited.insert((namespace, name));
                    }
                    continue;
                }
                if not_inherited.contains(&(namespace, name))
                    || (track && !own.insert((namespace, name)))
                {
                    continue;
                }
                let attr_key = resolved.and_then(|r| r.resolved_ref);
                let decl = attr_key.and_then(|k| self.schema_set.arenas.attributes.get(k));
                let (value, value_source) = self.effective_value_constraint(attr_use, attr_key);
                visit(&AttributeUseEntry {
                    name,
                    namespace,
                    use_kind: attr_use.use_kind,
                    attr_key,
                    type_key: resolved
                        .and_then(|r| r.resolved_type)
                        .or_else(|| decl.and_then(|d| d.resolved_type)),
                    value,
                    value_source,
                    #[cfg(feature = "xsd11")]
                    inheritable: attr_use.use_inheritable(decl),
                });
            }

            for ga in &groups {
                if ga.use_kind == AttributeUseKind::Prohibited
                    || not_inherited.contains(&(ga.namespace, ga.name))
                    || !own.insert((ga.namespace, ga.name))
                {
                    continue;
                }
                visit(&AttributeUseEntry {
                    name: ga.name,
                    namespace: ga.namespace,
                    use_kind: ga.use_kind,
                    attr_key: ga.attr_key,
                    type_key: ga.type_key,
                    value: ga.value.as_deref(),
                    value_source: ga.value_source.as_ref(),
                    #[cfg(feature = "xsd11")]
                    inheritable: ga.inheritable,
                });
            }

            if base.is_some() {
                // Clause 3: an extension inherits every use of its base (3.1:
                // its own names cannot repeat them, and a prohibition there is
                // ignored); a restriction none of the names it includes or
                // prohibits (3.2).
                not_inherited.absorb(&mut own);
                if !matches!(
                    ct_data.derivation_method,
                    Some(crate::parser::frames::DerivationMethod::Extension)
                ) {
                    not_inherited.absorb(&mut prohibited);
                }
            }
            current = base;
        }
    }

    /// Resolve the effective name and namespace for an attribute use.
    ///
    /// For inline attributes, returns the name directly from the use.
    /// For `<xs:attribute ref="..."/>`, resolves through the global declaration.
    ///
    /// `fallback_namespace` is used when the attribute's source document is
    /// unavailable (e.g. synthesized attributes); callers should pass the
    /// containing type's or group's target namespace.
    fn resolve_attr_use_name_ns(
        &self,
        attr_use: &AttributeUseResult,
        resolved: Option<&ResolvedAttributeUse>,
        fallback_namespace: Option<NameId>,
    ) -> (NameId, Option<NameId>) {
        if let Some(name) = attr_use.attribute.name {
            if attr_use.attribute.ref_name.is_none() {
                // Inline local attribute: apply form / attributeFormDefault
                let ns = self.schema_set.effective_local_attribute_namespace(
                    attr_use.attribute.target_namespace,
                    attr_use.attribute.form.as_deref(),
                    attr_use.attribute.source.as_ref(),
                    fallback_namespace,
                );
                return (name, ns);
            }
            // ref with name — fall through to ref resolution
        }
        if let Some(attr_key) = resolved.and_then(|r| r.resolved_ref) {
            if let Some(decl) = self.schema_set.arenas.attributes.get(attr_key) {
                if let Some(name) = decl.name {
                    return (name, decl.target_namespace);
                }
            }
        }
        (well_known::EMPTY, None)
    }

    // Effective-attribute-wildcard helpers superseded by
    // `crate::schema::derivation::compute_runtime_attribute_wildcard`,
    // which implements full §3.6.2.2 (own + groups intersection) plus
    // §3.4.2.5 (extension union over the base chain) in canonical form.

    /// The lexical form of an attribute use's ·effective value constraint·
    /// with the element it was written on. Structures §3.4.4.2: "The
    /// effective value constraint of an attribute use U is U.{value
    /// constraint}, if present, otherwise U.{attribute declaration}.{value
    /// constraint}, if present, otherwise the effective value constraint is
    /// ·absent·" — the use's own `default` / `fixed` first, for a reference
    /// in a type or in an attribute group alike.
    fn effective_value_constraint<'s>(
        &'s self,
        attr_use: &'s AttributeUseResult,
        attr_key: Option<AttributeKey>,
    ) -> (Option<&'s str>, Option<&'s SourceRef>) {
        let own = &attr_use.attribute;
        if let Some(value) = own.default_value.as_deref().or(own.fixed_value.as_deref()) {
            return (Some(value), own.source.as_ref());
        }
        match attr_key.and_then(|k| self.schema_set.arenas.attributes.get(k)) {
            Some(decl) => match decl
                .default_value
                .as_deref()
                .or(decl.fixed_value.as_deref())
            {
                Some(value) => (Some(value), decl.source.as_ref()),
                None => (None, None),
            },
            None => (None, None),
        }
    }

    /// An attribute use's fixed value — its own, otherwise its declaration's —
    /// with the source of the element it was written on.
    fn attribute_use_fixed(
        &self,
        attr_use: &crate::parser::frames::AttributeUseResult,
        attr_key: Option<AttributeKey>,
    ) -> (Option<String>, Option<SourceRef>) {
        if let Some(fixed) = &attr_use.attribute.fixed_value {
            return (Some(fixed.clone()), attr_use.attribute.source.clone());
        }
        match attr_key.and_then(|k| self.schema_set.arenas.attributes.get(k)) {
            Some(decl) => (decl.fixed_value.clone(), decl.source.clone()),
            None => (None, None),
        }
    }

    /// Look the attribute (`local_name`, `namespace`) up among the {attribute
    /// uses} of the complex type `ct_data` (Structures §3.4.2.4), by the rule
    /// of [`for_each_attribute_use`](Self::for_each_attribute_use): at each
    /// level of the base chain, the type's own `<attribute>` uses, then its
    /// attribute groups' uses; a prohibited `<attribute>` of that name keeps
    /// the use from being inherited only when the level is a restriction
    /// (clause 3.2.2) — it never hides a use of the type itself, an
    /// extension's is ignored (clause 3.1), and a group's prohibited
    /// `<attribute>` is no use at all (W3C Bugzilla #4043).
    ///
    /// `Prohibited` means a restriction removed the name (the caller's
    /// wildcard rescue may still admit the attribute), `NotFound` that no
    /// level declares or removes it.
    fn find_attribute_in_type(
        &self,
        ct_data: &ComplexTypeDefData,
        local_name: NameId,
        namespace: Option<NameId>,
    ) -> AttributeLookup {
        let any_type = self.schema_set.any_type_key();
        let mut ct_data = ct_data;
        loop {
            let mut prohibited_here = false;
            for (i, attr_use) in ct_data.attributes.iter().enumerate() {
                let resolved = ct_data.resolved_attributes.get(i);
                let (attr_name, attr_ns) =
                    self.resolve_attr_use_name_ns(attr_use, resolved, ct_data.target_namespace);
                if attr_name != local_name || attr_ns != namespace {
                    continue;
                }
                let attr_key = resolved.and_then(|r| r.resolved_ref);
                let attr_type = resolved.and_then(|r| r.resolved_type).or_else(|| {
                    attr_key
                        .and_then(|k| self.schema_set.arenas.attributes.get(k))
                        .and_then(|d| d.resolved_type)
                });

                // Get fixed value from the attribute use or from the attribute declaration
                let (fixed, fixed_source) = self.attribute_use_fixed(attr_use, attr_key);
                let inheritable = attr_use.use_inheritable(
                    attr_key.and_then(|k| self.schema_set.arenas.attributes.get(k)),
                );

                if attr_use.use_kind == AttributeUseKind::Prohibited {
                    // In XSD 1.0, use="prohibited" combined with fixed=X is a valid
                    // schema construct (constraint au-props-correct.5 was added in XSD 1.1).
                    // The combination means the attribute may appear with the fixed value.
                    // (W3C test attP031: schema valid in 1.0, instance with fixed value valid.)
                    if fixed.is_some() && self.schema_set.is_xsd10() {
                        return AttributeLookup::Found(
                            attr_key,
                            attr_type,
                            fixed,
                            fixed_source,
                            inheritable,
                        );
                    }
                    prohibited_here = true;
                    continue;
                }
                return AttributeLookup::Found(
                    attr_key,
                    attr_type,
                    fixed,
                    fixed_source,
                    inheritable,
                );
            }

            // Search attribute groups. A prohibited use inside an attribute
            // group is transparent — it does NOT propagate the prohibition to
            // the referencing type (W3C Bugzilla #4043 / TSTF conclusion).
            for ga in self.collect_group_attributes(ct_data) {
                if ga.name == local_name
                    && ga.namespace == namespace
                    && ga.use_kind != AttributeUseKind::Prohibited
                {
                    #[cfg(feature = "xsd11")]
                    let inheritable = ga.inheritable;
                    #[cfg(not(feature = "xsd11"))]
                    let inheritable = false;
                    return AttributeLookup::Found(
                        ga.attr_key,
                        ga.type_key,
                        ga.fixed_value,
                        ga.fixed_source,
                        inheritable,
                    );
                }
            }

            // Clause 3: the use is inherited from the base unless a
            // restriction prohibits the name (an extension's prohibition is
            // ignored).
            let extension = matches!(
                ct_data.derivation_method,
                Some(crate::parser::frames::DerivationMethod::Extension)
            );
            if prohibited_here && !extension {
                return AttributeLookup::Prohibited;
            }
            match ct_data.resolved_base_type {
                Some(TypeKey::Complex(base)) if base != any_type => {
                    ct_data = &self.schema_set.arenas.complex_types[base];
                }
                _ => return AttributeLookup::NotFound,
            }
        }
    }

    /// Record inheritable attributes with default/fixed values for propagation
    /// to descendant elements (XSD 1.1 §3.3.5.6).
    ///
    /// The ·defaulted attributes· (§3.4.4.2) of the element join its
    /// \[attributes\] (§3.4.5.1), so an inheritable one is potentially
    /// inherited like one the instance carried. Walks the type's {attribute
    /// uses} — its own, its attribute groups', and those inherited from its
    /// base types ([`for_each_attribute_use`](Self::for_each_attribute_use)) —
    /// and records the optional inheritable ones with a value constraint that
    /// were not explicitly provided.
    #[cfg(feature = "xsd11")]
    fn record_inheritable_defaults(&mut self, ct_key: ComplexTypeKey) {
        use super::context::InheritedAttributeValue;

        // Collect candidates to avoid borrow conflict with validation_stack
        let mut candidates: Vec<(Option<NameId>, NameId, String, Option<AttributeKey>)> =
            Vec::new();
        self.for_each_attribute_use(ct_key, |u| {
            if !u.inheritable || u.use_kind != AttributeUseKind::Optional {
                return;
            }
            if let Some(val) = u.value {
                candidates.push((u.namespace, u.name, val.to_string(), u.attr_key));
            }
        });

        // Apply to outgoing_inherited: only for attributes not explicitly
        // provided. Use insert() (not or_insert) so defaulted values from
        // this element shadow ancestor values per nearest-owner rule.
        if let Some(ev) = self.validation_stack.last_mut() {
            for (ns, name, val, attr_key) in candidates {
                if !ev.seen_attributes.contains(&(ns, name)) {
                    ev.outgoing_inherited.insert(
                        (ns, name),
                        InheritedAttributeValue {
                            value: val,
                            attribute_key: attr_key,
                        },
                    );
                }
            }
        }
    }
}

/// Resolve an `xml:base` value against an inherited base URI.
///
/// If the value is already absolute (contains `://` or starts with `/`
/// or is a Windows absolute path), it replaces the inherited base.
/// Otherwise, it is resolved as a relative URI against the inherited base.
fn resolve_base_uri(xml_base: &str, inherited: &str) -> String {
    if xml_base.is_empty() {
        return inherited.to_string();
    }
    // Check for absolute URI
    if xml_base.contains("://")
        || xml_base.starts_with('/')
        || (xml_base.len() >= 2 && xml_base.as_bytes().get(1) == Some(&b':'))
    {
        return xml_base.to_string();
    }
    if inherited.is_empty() {
        return xml_base.to_string();
    }
    // Resolve relative against inherited base directory.
    // Find the last path separator (handles both / and \ for Windows paths).
    let last_sep = inherited.rfind('/').or_else(|| inherited.rfind('\\'));
    let base_dir = match last_sep {
        Some(pos) => &inherited[..=pos],
        None => "",
    };
    if base_dir.is_empty() {
        xml_base.to_string()
    } else {
        format!("{}{}", base_dir, xml_base)
    }
}

/// Resolve the `xs:QName` / `xs:NOTATION` members of a list value — the value
/// itself, or the list a union's chosen member type gave — against the
/// instance namespace context `ctx`. Datatypes §3.3.18: the QName lexical
/// mapping "depends on the namespace bindings in scope where the literal
/// occurs"; the simple-type validator has none, so each such member comes back
/// as its lexical form under the QName / NOTATION code.
///
/// `Ok(Some(v))` is the value with those members resolved; `Ok(None)` means
/// nothing needed resolving; `Err(prefix)` names the first member prefix that
/// `ctx` binds nowhere (`cvc-datatype-valid.1.2.1`, as for an atomic value).
fn resolve_list_qname_members(
    value: &XmlValue,
    ctx: &NamespaceContextSnapshot,
    names: &crate::namespace::table::NameTable,
) -> Result<Option<XmlValue>, String> {
    use crate::types::value::{QNameValue, XmlAtomicValue, XmlValueKind};
    match &value.value {
        XmlValueKind::Union(inner) => Ok(resolve_list_qname_members(inner, ctx, names)?.map(
            |resolved| XmlValue {
                type_code: value.type_code,
                schema_type: value.schema_type,
                value: XmlValueKind::Union(Box::new(resolved)),
            },
        )),
        XmlValueKind::List { item_type, items } => {
            let mut resolved: Option<Vec<XmlValue>> = None;
            for (index, member) in items.iter().enumerate() {
                let lexical = match &member.value {
                    XmlValueKind::Atomic(XmlAtomicValue::String(s))
                        if matches!(
                            member.type_code,
                            XmlTypeCode::QName | XmlTypeCode::Notation
                        ) =>
                    {
                        s
                    }
                    _ => continue,
                };
                let qn = match parse_qname_with_snapshot(lexical, ctx, names, true) {
                    Ok(qn) => QNameValue::new(qn, names),
                    Err(QNameError::UndefinedPrefix(prefix)) => return Err(prefix),
                    // Lexical errors were the simple-type validator's to report.
                    Err(_) => continue,
                };
                let atom = if member.type_code == XmlTypeCode::Notation {
                    XmlAtomicValue::Notation(qn)
                } else {
                    XmlAtomicValue::QName(qn)
                };
                let members = resolved.get_or_insert_with(|| items.clone());
                members[index] = XmlValue {
                    type_code: member.type_code,
                    schema_type: member.schema_type,
                    value: XmlValueKind::Atomic(atom),
                };
            }
            Ok(resolved.map(|items| XmlValue {
                type_code: value.type_code,
                schema_type: value.schema_type,
                value: XmlValueKind::List {
                    item_type: *item_type,
                    items,
                },
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
