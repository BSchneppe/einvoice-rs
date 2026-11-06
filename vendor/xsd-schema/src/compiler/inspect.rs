//! Readable, source-attributed reports for compiled content models.
//!
//! A complex type's content model passes through three representations before
//! it validates anything: the particles the schema author wrote, the matcher
//! the compiler builds from them, and the frontier the runtime executes. When
//! a model behaves unexpectedly — a counter where an unrolled range was
//! expected, a wildcard that swallows a declared element, a substitution group
//! that admits more names than it looks like it should — the only way to see
//! which of the three disagrees with the others used to be a debugger.
//!
//! [`inspect_content_model`] produces a [`ContentModelReport`] with those three
//! views side by side:
//!
//! * **Source** — the type's expanded name, the schema document and line/column
//!   it was written at, its content type, mixed flag, derivation, the effective
//!   open content, and the XSD version the schema set runs in.
//! * **Authored particles** — an indented tree of the *resolved* content
//!   particles as the schema wrote them: compositors, group references
//!   (expanded inline but labelled as references), element particles with their
//!   resolved declaration and type, and wildcards with their namespace
//!   constraint. Every node carries `minOccurs..maxOccurs` and says whether
//!   that range will be **unrolled** (`maxOccurs` ≤ `COUNTED_THRESHOLD`) or
//!   compiled to a **counter**. For an extension, the base type's contribution
//!   is shown first, in the order the compiler concatenates it.
//! * **Compiled** — the matcher the validator actually runs: the matcher kind,
//!   state and transition counts, counter definitions, the initial frontier
//!   variant [`ActiveStates::try_from_nfa`] picks (or the execution limit it
//!   hits), a full state table with per-state origins, and an *exposure* line
//!   naming the models that can reach [`MAX_ACTIVE_CONFIGS`].
//!
//! # The compiled view is what runs
//!
//! [`inspect_content_model`] compiles through [`compile_content_model_matcher`]
//! — the same function
//! [`SchemaValidator`](crate::validation::SchemaValidator) calls at
//! construction — and derives the frontier through
//! [`ActiveStates::try_from_nfa`], the same constructor the runtime uses. It
//! runs no alternative path and applies no simplification, so the state table
//! is the automaton validation executes, state ids included.
//!
//! [`SchemaValidator::describe_content_model`](crate::validation::SchemaValidator::describe_content_model)
//! goes one better: it reads the *already prepared* model out of the validator
//! rather than recompiling, so the report is that exact object. For a type
//! listed in
//! [`content_model_failures`](crate::validation::SchemaValidator::content_model_failures)
//! it reports the preparation failure and its reason in place of a compiled
//! view.
//!
//! # Example
//!
//! ```
//! use xsd_schema::compiler::{find_complex_type, inspect_content_model};
//! use xsd_schema::SchemaSetBuilder;
//!
//! let schema_set = SchemaSetBuilder::new()
//!     .add_source(
//!         r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
//!              <xs:complexType name="Pair">
//!                <xs:sequence>
//!                  <xs:element name="a" type="xs:string"/>
//!                  <xs:element name="b" type="xs:string" minOccurs="0"/>
//!                </xs:sequence>
//!              </xs:complexType>
//!            </xs:schema>"#,
//!         "file:///pair.xsd",
//!     )
//!     .expect("schema source accepted")
//!     .compile()
//!     .expect("schema compiles")
//!     .into_schema_set();
//!
//! let key = find_complex_type(&schema_set, None, "Pair").expect("named type");
//! let report = inspect_content_model(&schema_set, key).expect("model compiles");
//! let text = report.to_string();
//! assert!(text.contains("Authored particles"));
//! assert!(text.contains("element a"));
//! assert!(text.contains("[unrolled]"));
//! ```

use std::fmt;

use crate::arenas::ComplexTypeDefData;
use crate::ids::{ComplexTypeKey, ElementKey, ModelGroupKey, NameId, TypeKey};
use crate::namespace::XS_NAMESPACE;
use crate::parser::frames::{
    ComplexContentResult, Compositor, DerivationMethod, ElementFrameResult, ModelGroupDefResult,
    NamespaceToken, NotQNameItem, ParticleResult, ParticleTerm, TypeRefResult, WildcardNamespace,
    WildcardResult,
};
use crate::parser::location::SourceRef;
use crate::schema::model::XsdVersion;
use crate::schema::SchemaSet;
use crate::types::complex::{
    NamespaceConstraint, OpenContentMode as TypesOpenContentMode, ProcessContents, WildcardRef,
};
use crate::validation::content::CompiledContentModel;
use crate::validation::info::ContentType;

use super::all_group::{AllGroupModel, OpenContentMode as AllGroupOpenContentMode};
use super::error::NfaCompileError;
use super::nfa::{
    ActiveStates, CounterDef, CounterId, NfaTable, NfaTerm, StateId, TransitionKind,
    MAX_ACTIVE_CONFIGS,
};
use super::particle::COUNTED_THRESHOLD;
use super::substitution::{build_substitution_group_map, SubstitutionGroupMap};
use super::{compile_content_model_matcher, ContentModelMatcher};

/// Guard against pathological base-type chains while collecting authored views.
const MAX_REPORT_DEPTH: usize = 100;

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Look up a **named global complex type** by its expanded name.
///
/// `namespace` is the namespace URI, or `None` for the absent namespace. Only
/// named, globally declared complex types can be found this way — an anonymous
/// local type has no expanded name, and its [`ComplexTypeKey`] must be reached
/// through the declaration that owns it (an element declaration's
/// `resolved_type`, for instance).
///
/// Returns `None` when the name is not interned in this schema set, resolves to
/// no type, or resolves to a *simple* type.
pub fn find_complex_type(
    schema_set: &SchemaSet,
    namespace: Option<&str>,
    local: &str,
) -> Option<ComplexTypeKey> {
    let ns = match namespace {
        Some(ns) => Some(schema_set.name_table.get(ns)?),
        None => None,
    };
    let local_id = schema_set.name_table.get(local)?;
    schema_set.lookup_type(ns, local_id)?.as_complex()
}

/// Compile a complex type's content model and describe it in three views.
///
/// Compilation goes through [`compile_content_model_matcher`], the same entry
/// point the validator uses, so the **Compiled** view is exactly the matcher
/// validation would execute for this type.
///
/// Returns the compiler's error when the model does not compile. A model that
/// compiles but whose initial frontier exceeds an execution limit is *not* an
/// error here: the report is produced and its frontier line names the limit.
///
/// # Panics
///
/// Panics if `ct_key` does not belong to `schema_set`, matching the indexing
/// convention the rest of the crate uses for arena keys.
pub fn inspect_content_model(
    schema_set: &SchemaSet,
    ct_key: ComplexTypeKey,
) -> Result<ContentModelReport, NfaCompileError> {
    let ct_data = &schema_set.arenas.complex_types[ct_key];
    let subst = build_substitution_group_map(schema_set);
    let subst = (!subst.is_empty()).then_some(subst);
    let matcher = compile_content_model_matcher(schema_set, ct_data)?;
    let compiled = compiled_view_from_matcher(schema_set, subst.as_ref(), &matcher);
    Ok(assemble(schema_set, ct_data, subst.as_ref(), compiled))
}

/// Build a report for a type whose model the validator already prepared (or
/// failed to prepare). Backs
/// [`SchemaValidator::describe_content_model`](crate::validation::SchemaValidator::describe_content_model).
pub(crate) fn report_from_prepared(
    schema_set: &SchemaSet,
    ct_key: ComplexTypeKey,
    prepared: Option<&CompiledContentModel>,
    failure: Option<&str>,
    subst: Option<&SubstitutionGroupMap>,
) -> Option<ContentModelReport> {
    let ct_data = schema_set.arenas.get_complex_type(ct_key)?;
    let compiled = match (failure, prepared) {
        (Some(reason), _) => CompiledView::Failed {
            reason: reason.to_string(),
        },
        (None, Some(model)) => compiled_view_from_prepared(schema_set, subst, model),
        // No model prepared and no failure recorded: the type's content type is
        // neither element-only nor mixed, so the validator never builds one.
        // Compile on the spot so the view still shows what this type would get.
        (None, None) => match compile_content_model_matcher(schema_set, ct_data) {
            Ok(matcher) => {
                let mut view = compiled_view_from_matcher(schema_set, subst, &matcher);
                view.mark_not_prepared();
                view
            }
            Err(err) => CompiledView::Failed {
                reason: err.to_string(),
            },
        },
    };
    Some(assemble(schema_set, ct_data, subst, compiled))
}

fn assemble(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
    subst: Option<&SubstitutionGroupMap>,
    compiled: CompiledView,
) -> ContentModelReport {
    let source = source_view(schema_set, ct_data, &compiled);
    let authored = authored_view(schema_set, ct_data, subst, &compiled);
    ContentModelReport {
        source,
        authored,
        compiled,
    }
}

// ---------------------------------------------------------------------------
// Report structure
// ---------------------------------------------------------------------------

/// A three-view description of one complex type's content model.
///
/// See the [module documentation](self) for what each view contains. The
/// [`Display`](fmt::Display) rendering is stable and deterministic: every set
/// borrowed from a hash map is sorted before it reaches the output.
#[derive(Debug, Clone)]
pub struct ContentModelReport {
    /// Where the type came from and what kind of content it declares.
    pub source: SourceView,
    /// The resolved particle tree as the schema wrote it.
    pub authored: AuthoredView,
    /// The matcher the validator executes.
    pub compiled: CompiledView,
}

/// Identity and declaration-level facts about the inspected type.
#[derive(Debug, Clone)]
pub struct SourceView {
    /// `{namespace}local`, or `(anonymous)` for an unnamed local type.
    pub type_name: String,
    /// Base URI of the schema document, when a location is available.
    pub document: Option<String>,
    /// 1-based line of the type definition, when a location is available.
    pub line: Option<usize>,
    /// 1-based column of the type definition, when a location is available.
    pub column: Option<usize>,
    /// Content type as the validator determines it.
    pub content_type: ContentType,
    /// The type's `mixed` flag as declared.
    pub mixed: bool,
    /// `extension of {ns}Base` / `restriction of {ns}Base`, when derived.
    pub derivation: Option<String>,
    /// Effective open content carried by the compiled matcher (XSD 1.1).
    pub open_content: Option<OpenContentView>,
    /// XSD version the schema set is configured for.
    pub xsd_version: XsdVersion,
}

/// The authored particle tree, in compiler concatenation order.
#[derive(Debug, Clone, Default)]
pub struct AuthoredView {
    /// One section per contributing type: for an extension chain the
    /// outermost base comes first and the inspected type's own particle last,
    /// matching the order [`compile_content_model_matcher`] concatenates them.
    pub sections: Vec<AuthoredSection>,
}

/// One contributing type's authored particle.
#[derive(Debug, Clone)]
pub struct AuthoredSection {
    /// `None` for the inspected type's own content; `Some(name)` when this
    /// section is inherited from a base type through extension.
    pub from_base_type: Option<String>,
    /// The root particle, or `None` when the type declares no particle.
    pub root: Option<AuthoredNode>,
}

/// One node of the authored particle tree.
#[derive(Debug, Clone)]
pub struct AuthoredNode {
    /// What this particle is.
    pub kind: AuthoredKind,
    /// `minOccurs`.
    pub min_occurs: u32,
    /// `maxOccurs`; `None` is `unbounded`.
    pub max_occurs: Option<u32>,
    /// How the compiler realises this occurrence range.
    pub occurrence: OccurrenceCompilation,
    /// `file:line:column`, when the particle carries a source location.
    pub origin: Option<String>,
    /// Child particles (compositors and expanded group references).
    pub children: Vec<AuthoredNode>,
}

/// The kind of an authored particle.
#[derive(Debug, Clone)]
pub enum AuthoredKind {
    /// `xs:sequence`.
    Sequence,
    /// `xs:choice`.
    Choice,
    /// `xs:all`.
    All,
    /// `xs:group ref="..."`, expanded inline below this node.
    GroupRef(GroupRefView),
    /// An element particle.
    Element(ElementTermView),
    /// An `xs:any` wildcard.
    Wildcard(WildcardTermView),
}

/// A resolved named model group reference.
#[derive(Debug, Clone)]
pub struct GroupRefView {
    /// `{namespace}local` of the referenced group.
    pub name: String,
    /// Compositor of the referenced group, when it resolved.
    pub compositor: Option<Compositor>,
    /// `false` when the reference could not be resolved in this schema set.
    pub resolved: bool,
    /// `true` when expansion stopped because the group references itself.
    pub recursive: bool,
}

/// How the compiler realises an occurrence range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceCompilation {
    /// `1..1`: nothing to repeat.
    Single,
    /// `maxOccurs` ≤ `COUNTED_THRESHOLD` (or unbounded with a small
    /// `minOccurs`): the fragment is cloned into the automaton.
    Unrolled,
    /// Above the threshold: counter transitions enforce the bound exactly.
    Counted,
    /// Inside an all-group model: the bound is enforced by the all-group's own
    /// per-particle counter, not by the NFA.
    AllGroupCounter,
}

impl fmt::Display for OccurrenceCompilation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            OccurrenceCompilation::Single => "single",
            OccurrenceCompilation::Unrolled => "unrolled",
            OccurrenceCompilation::Counted => "counter",
            OccurrenceCompilation::AllGroupCounter => "all-group counter",
        };
        f.write_str(s)
    }
}

/// The matcher the validator executes for this type.
#[derive(Debug, Clone)]
pub enum CompiledView {
    /// An NFA, possibly carrying an open-content wildcard.
    Nfa(NfaView),
    /// An all-group model (unordered particles).
    AllGroup(AllGroupView),
    /// The validator could not prepare a model for this type.
    Failed {
        /// The compiler error or execution-limit message.
        reason: String,
    },
}

impl CompiledView {
    /// Note that the validator holds no prepared model for this type, so the
    /// view below was compiled on demand.
    fn mark_not_prepared(&mut self) {
        let note = "compiled on demand: the validator prepares no model for this content type";
        match self {
            CompiledView::Nfa(view) => view.note = Some(note.to_string()),
            CompiledView::AllGroup(view) => view.note = Some(note.to_string()),
            CompiledView::Failed { .. } => {}
        }
    }

    /// The matcher kind, in the vocabulary the compiler uses.
    pub fn matcher_kind(&self) -> &str {
        match self {
            CompiledView::Nfa(view) => &view.matcher,
            CompiledView::AllGroup(view) => &view.matcher,
            CompiledView::Failed { .. } => "(preparation failed)",
        }
    }
}

/// A compiled NFA content model.
#[derive(Debug, Clone)]
pub struct NfaView {
    /// `NFA`, or `NFA + open content (interleave|suffix)`.
    pub matcher: String,
    /// Number of states in the table.
    pub state_count: usize,
    /// Total number of transitions across all states.
    pub transition_count: usize,
    /// Counter definitions, in counter-id order.
    pub counters: Vec<CounterView>,
    /// The initial frontier variant, or the limit it exceeded.
    pub frontier: FrontierView,
    /// One row per state, in state-id order.
    pub states: Vec<StateRow>,
    /// Substitution groups reachable from this model's element terms, sorted.
    pub substitution_groups: Vec<SubstitutionGroupView>,
    /// Effective open content, when the matcher carries one.
    pub open_content: Option<OpenContentView>,
    /// Whether this model can reach [`MAX_ACTIVE_CONFIGS`].
    pub exposure: ExposureView,
    /// Free-form note about how this view was obtained.
    pub note: Option<String>,
}

/// One counted loop of an NFA.
#[derive(Debug, Clone, Copy)]
pub struct CounterView {
    /// Counter id; printed as `c{id}`.
    pub id: CounterId,
    /// Minimum completed iterations required to exit the loop.
    pub min: u32,
    /// Maximum iterations the loop may run.
    pub max: u32,
    /// Whether the loop body can be traversed without consuming input.
    pub body_nullable: bool,
}

/// The initial frontier [`ActiveStates::try_from_nfa`] produces.
#[derive(Debug, Clone)]
pub enum FrontierView {
    /// Counter-free NFA: a plain state bitset.
    Simple {
        /// States in the start-state epsilon closure.
        states: usize,
    },
    /// Scalar counted configurations.
    Counted {
        /// Configurations in the initial closure.
        configs: usize,
    },
    /// Single nullable counter collapsed into per-state ranges.
    RangedSingle {
        /// States carrying a counter range.
        states: usize,
    },
    /// One ranged counter plus scalar companions.
    Hybrid {
        /// Configurations in the initial closure.
        configs: usize,
        /// Index of the counter chosen for ranging.
        ranged_counter: usize,
    },
    /// The initial closure itself exceeded an execution limit.
    LimitExceeded {
        /// The limit message.
        message: String,
    },
}

impl fmt::Display for FrontierView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrontierView::Simple { states } => write!(f, "Simple({states} states)"),
            FrontierView::Counted { configs } => write!(f, "Counted({configs} configs)"),
            FrontierView::RangedSingle { states } => write!(f, "RangedSingle({states} states)"),
            FrontierView::Hybrid {
                configs,
                ranged_counter,
            } => write!(
                f,
                "Hybrid(ranged counter c{ranged_counter}, {configs} configs)"
            ),
            FrontierView::LimitExceeded { message } => write!(f, "unavailable: {message}"),
        }
    }
}

/// One row of the NFA state table.
#[derive(Debug, Clone)]
pub struct StateRow {
    /// State id, as used inside the automaton.
    pub id: StateId,
    /// `true` for the start state.
    pub start: bool,
    /// `true` for the accepting state.
    pub accept: bool,
    /// The term a consuming transition into this state must match.
    pub term: Option<TermView>,
    /// Outgoing transitions, in declaration order.
    pub transitions: Vec<String>,
    /// `file:line:column`, when the state carries an origin.
    pub origin: Option<String>,
}

/// An element or wildcard term.
#[derive(Debug, Clone)]
pub enum TermView {
    /// A specific element.
    Element(ElementTermView),
    /// A wildcard.
    Wildcard(WildcardTermView),
}

impl fmt::Display for TermView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TermView::Element(e) => write!(f, "{e}"),
            TermView::Wildcard(w) => write!(f, "{w}"),
        }
    }
}

/// An element term with its resolved bindings.
#[derive(Debug, Clone)]
pub struct ElementTermView {
    /// `{namespace}local`.
    pub name: String,
    /// How the declaration resolved.
    pub declaration: DeclarationBinding,
    /// The governing type's name, when it resolved.
    pub type_name: Option<String>,
    /// Other element names this term accepts through substitution, sorted.
    /// Empty when the term matches only its own name.
    pub substitutable: Vec<String>,
}

impl fmt::Display for ElementTermView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "element {}", self.name)?;
        if let Some(ty) = &self.type_name {
            write!(f, " : {ty}")?;
        }
        write!(f, " [{}", self.declaration)?;
        if !self.substitutable.is_empty() {
            write!(f, ", +{} substitutable", self.substitutable.len() - 1)?;
        }
        f.write_str("]")
    }
}

/// How an element particle's declaration resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationBinding {
    /// Resolved to a global (top-level) element declaration.
    Global,
    /// Resolved to a local element declaration inside this content model.
    Local,
    /// No declaration key: the term matches by name only.
    Unresolved,
}

impl fmt::Display for DeclarationBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DeclarationBinding::Global => "global decl",
            DeclarationBinding::Local => "local decl",
            DeclarationBinding::Unresolved => "no decl",
        };
        f.write_str(s)
    }
}

/// A wildcard term.
#[derive(Debug, Clone)]
pub struct WildcardTermView {
    /// Namespace constraint, in schema spelling (`##any`, `##other`, a list…).
    pub namespace: String,
    /// `strict` / `lax` / `skip`.
    pub process_contents: &'static str,
    /// Excluded QNames (XSD 1.1 `notQName`), sorted.
    pub not_qnames: Vec<String>,
}

impl fmt::Display for WildcardTermView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "any {} [processContents={}]",
            self.namespace, self.process_contents
        )?;
        if !self.not_qnames.is_empty() {
            write!(f, " notQName={{{}}}", self.not_qnames.join(" "))?;
        }
        Ok(())
    }
}

/// A substitution group reachable from a content model's element terms.
#[derive(Debug, Clone)]
pub struct SubstitutionGroupView {
    /// The head element's `{namespace}local`.
    pub head: String,
    /// Every name the head's term accepts, sorted (the head included when it
    /// is not abstract).
    pub accepts: Vec<String>,
}

/// A compiled all-group model.
#[derive(Debug, Clone)]
pub struct AllGroupView {
    /// `all-group`, or `all-group + open content (interleave|suffix)`.
    pub matcher: String,
    /// `true` when the whole group is optional (`minOccurs="0"` outside).
    pub outer_optional: bool,
    /// Members, in declaration order.
    pub members: Vec<AllMemberView>,
    /// Members with `minOccurs > 0`, in declaration order.
    pub required: Vec<String>,
    /// Substitution groups reachable from the members, sorted.
    pub substitution_groups: Vec<SubstitutionGroupView>,
    /// Open content carried by the model.
    pub open_content: Option<OpenContentView>,
    /// Free-form note about how this view was obtained.
    pub note: Option<String>,
}

/// One member of an all-group.
#[derive(Debug, Clone)]
pub struct AllMemberView {
    /// The member's term.
    pub term: TermView,
    /// `minOccurs`.
    pub min_occurs: u32,
    /// `maxOccurs`; `None` is `unbounded`.
    pub max_occurs: Option<u32>,
    /// `file:line:column`, when the member carries an origin.
    pub origin: Option<String>,
}

/// Effective open content of a content model (XSD 1.1).
#[derive(Debug, Clone)]
pub struct OpenContentView {
    /// `none` / `interleave` / `suffix`.
    pub mode: &'static str,
    /// The open-content wildcard, when one is present.
    pub wildcard: Option<WildcardTermView>,
}

impl fmt::Display for OpenContentView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.mode)?;
        if let Some(w) = &self.wildcard {
            write!(f, ", {w}")?;
        }
        Ok(())
    }
}

/// Whether this model can hit [`MAX_ACTIVE_CONFIGS`] during validation.
#[derive(Debug, Clone)]
pub struct ExposureView {
    /// `true` when the NFA has no counters at all.
    pub counter_free: bool,
    /// Total number of counters.
    pub counter_count: usize,
    /// Counters whose loop body is nullable — the shape that multiplies
    /// configurations.
    pub nullable_counters: Vec<CounterId>,
}

impl fmt::Display for ExposureView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.counter_free {
            return write!(
                f,
                "counter-free — one configuration per state, MAX_ACTIVE_CONFIGS ({MAX_ACTIVE_CONFIGS}) unreachable"
            );
        }
        write!(f, "{} counter(s)", self.counter_count)?;
        if self.nullable_counters.is_empty() {
            write!(
                f,
                ", no nullable body — configurations stay bounded by the loop structure"
            )
        } else {
            let list = self
                .nullable_counters
                .iter()
                .map(|c| format!("c{c}"))
                .collect::<Vec<_>>()
                .join(", ");
            write!(
                f,
                ", nullable body in {list} — this model can reach MAX_ACTIVE_CONFIGS ({MAX_ACTIVE_CONFIGS})"
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Source view
// ---------------------------------------------------------------------------

fn source_view(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
    compiled: &CompiledView,
) -> SourceView {
    let type_name = match ct_data.name {
        Some(name) => qname(schema_set, ct_data.target_namespace, name),
        None => "(anonymous)".to_string(),
    };
    let location = schema_set.locate(ct_data.source.as_ref());
    let derivation = ct_data.derivation_method.map(|method| {
        let verb = match method {
            DerivationMethod::Extension => "extension",
            DerivationMethod::Restriction => "restriction",
        };
        match ct_data.resolved_base_type {
            Some(key) => format!("{verb} of {}", type_key_name(schema_set, key)),
            None => format!("{verb} of (unresolved base)"),
        }
    });
    let open_content = match compiled {
        CompiledView::Nfa(view) => view.open_content.clone(),
        CompiledView::AllGroup(view) => view.open_content.clone(),
        CompiledView::Failed { .. } => None,
    };
    SourceView {
        type_name,
        document: location.as_ref().map(|l| l.base_uri.clone()),
        line: location.as_ref().map(|l| l.line),
        column: location.as_ref().map(|l| l.column),
        content_type: crate::validation::runtime::determine_content_type(schema_set, ct_data),
        mixed: ct_data.mixed,
        derivation,
        open_content,
        xsd_version: schema_set.xsd_version,
    }
}

// ---------------------------------------------------------------------------
// Authored view
// ---------------------------------------------------------------------------

fn authored_view(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
    subst: Option<&SubstitutionGroupMap>,
    compiled: &CompiledView,
) -> AuthoredView {
    let all_group_model = matches!(compiled, CompiledView::AllGroup(_));

    let mut sections = Vec::new();
    collect_sections(
        schema_set,
        ct_data,
        subst,
        all_group_model,
        None,
        0,
        &mut sections,
    );
    AuthoredView { sections }
}

fn collect_sections(
    schema_set: &SchemaSet,
    ct_data: &ComplexTypeDefData,
    subst: Option<&SubstitutionGroupMap>,
    all_group_model: bool,
    owner: Option<String>,
    depth: usize,
    out: &mut Vec<AuthoredSection>,
) {
    if depth >= MAX_REPORT_DEPTH {
        return;
    }
    // An extension prepends its base type's content, so the base's authored
    // particle is shown first — the order the compiler concatenates them in.
    if matches!(ct_data.derivation_method, Some(DerivationMethod::Extension)) {
        if let Some(TypeKey::Complex(base_key)) = ct_data.resolved_base_type {
            if let Some(base_data) = schema_set.arenas.get_complex_type(base_key) {
                let base_name = complex_type_name(schema_set, base_data);
                collect_sections(
                    schema_set,
                    base_data,
                    subst,
                    all_group_model,
                    Some(base_name),
                    depth + 1,
                    out,
                );
            }
        }
    }

    let particle = match &ct_data.content {
        ComplexContentResult::Complex(def) => def.particle.as_ref(),
        ComplexContentResult::Empty | ComplexContentResult::Simple(_) => None,
    };
    let root = particle.map(|p| {
        let mut walker = ParticleWalker {
            schema_set,
            target_namespace: ct_data.target_namespace,
            subst,
            all_group_model,
            flat_idx: 0,
            resolved_types: ct_data.resolved_content_particle_types.clone(),
            resolved_elements: ct_data.resolved_content_particle_elements.clone(),
            depth: 0,
            group_path: Vec::new(),
        };
        walker.walk(p, false)
    });
    out.push(AuthoredSection {
        from_base_type: owner,
        root,
    });
}

/// Walks authored particles in the same depth-first order the compiler does, so
/// the flat `resolved_content_particle_*` indices line up with the elements it
/// reports.
struct ParticleWalker<'a> {
    schema_set: &'a SchemaSet,
    target_namespace: Option<NameId>,
    subst: Option<&'a SubstitutionGroupMap>,
    /// `true` when the compiled matcher for this type is an all-group model,
    /// in which case the top-level group's members are counted by the
    /// all-group state rather than unrolled into an automaton.
    all_group_model: bool,
    flat_idx: usize,
    resolved_types: Vec<Option<TypeKey>>,
    resolved_elements: Vec<Option<ElementKey>>,
    depth: usize,
    group_path: Vec<ModelGroupKey>,
}

impl ParticleWalker<'_> {
    fn walk(&mut self, particle: &ParticleResult, all_mode: bool) -> AuthoredNode {
        let is_root = self.depth == 0;
        let origin = short_location(self.schema_set, particle.source.as_ref());
        let occurrence = if all_mode {
            OccurrenceCompilation::AllGroupCounter
        } else {
            classify_occurs(particle.min_occurs, particle.max_occurs)
        };

        let (kind, children) = match &particle.term {
            ParticleTerm::Element(elem) => (
                AuthoredKind::Element(self.element_term(elem, particle.source.as_ref())),
                Vec::new(),
            ),
            ParticleTerm::Any(wildcard) => (
                AuthoredKind::Wildcard(authored_wildcard(self.schema_set, wildcard)),
                Vec::new(),
            ),
            ParticleTerm::Group(group) => self.walk_group(group, is_root, all_mode),
        };

        AuthoredNode {
            kind,
            min_occurs: particle.min_occurs,
            max_occurs: particle.max_occurs,
            occurrence,
            origin,
            children,
        }
    }

    fn walk_group(
        &mut self,
        group: &ModelGroupDefResult,
        is_root: bool,
        all_mode: bool,
    ) -> (AuthoredKind, Vec<AuthoredNode>) {
        // A named reference: expand the referenced group inline, but keep the
        // node labelled as a reference and swap in the group's own flat
        // resolution arrays — exactly what `compile_model_group_data` does.
        if let Some(ref_name) = &group.ref_name {
            let name = qname(self.schema_set, ref_name.namespace, ref_name.local_name);
            let key = self
                .schema_set
                .lookup_model_group(ref_name.namespace, ref_name.local_name);
            let Some(key) = key else {
                return (
                    AuthoredKind::GroupRef(GroupRefView {
                        name,
                        compositor: None,
                        resolved: false,
                        recursive: false,
                    }),
                    Vec::new(),
                );
            };
            if self.group_path.contains(&key) || self.depth >= MAX_REPORT_DEPTH {
                return (
                    AuthoredKind::GroupRef(GroupRefView {
                        name,
                        compositor: None,
                        resolved: true,
                        recursive: true,
                    }),
                    Vec::new(),
                );
            }
            let Some(data) = self.schema_set.arenas.get_model_group(key) else {
                return (
                    AuthoredKind::GroupRef(GroupRefView {
                        name,
                        compositor: None,
                        resolved: false,
                        recursive: false,
                    }),
                    Vec::new(),
                );
            };

            let saved_flat = std::mem::replace(&mut self.flat_idx, 0);
            let saved_types = std::mem::replace(
                &mut self.resolved_types,
                data.resolved_particle_types.clone(),
            );
            let saved_elements = std::mem::replace(
                &mut self.resolved_elements,
                data.resolved_particle_elements.clone(),
            );
            self.group_path.push(key);
            let child_all = all_mode || (self.all_group_model && is_root);
            let children = self.walk_children(&data.particles, child_all);
            self.group_path.pop();
            self.flat_idx = saved_flat;
            self.resolved_types = saved_types;
            self.resolved_elements = saved_elements;

            return (
                AuthoredKind::GroupRef(GroupRefView {
                    name,
                    compositor: Some(data.compositor.unwrap_or(Compositor::Sequence)),
                    resolved: true,
                    recursive: false,
                }),
                children,
            );
        }

        let compositor = group.compositor.unwrap_or(Compositor::Sequence);
        let kind = match compositor {
            Compositor::Sequence => AuthoredKind::Sequence,
            Compositor::Choice => AuthoredKind::Choice,
            Compositor::All => AuthoredKind::All,
        };
        let child_all =
            all_mode || (self.all_group_model && is_root && compositor == Compositor::All);
        let children = self.walk_children(&group.particles, child_all);
        (kind, children)
    }

    fn walk_children(&mut self, particles: &[ParticleResult], all_mode: bool) -> Vec<AuthoredNode> {
        self.depth += 1;
        let children = particles
            .iter()
            .map(|p| self.walk(p, all_mode))
            .collect::<Vec<_>>();
        self.depth -= 1;
        children
    }

    /// Mirrors `CompileContext::build_element_term`: the same name/namespace
    /// resolution and the same flat depth-first index into the resolved arrays.
    fn element_term(
        &mut self,
        elem: &ElementFrameResult,
        source: Option<&SourceRef>,
    ) -> ElementTermView {
        let flat_idx = self.flat_idx;
        self.flat_idx += 1;

        let (name, namespace, element_key) = if let Some(ref_name) = &elem.ref_name {
            let key = self
                .schema_set
                .lookup_element(ref_name.namespace, ref_name.local_name);
            (ref_name.local_name, ref_name.namespace, key)
        } else if let Some(name) = elem.name {
            let source_ref = source.or(elem.source.as_ref());
            let namespace = self.schema_set.effective_local_element_namespace(
                elem.target_namespace,
                elem.form.as_deref(),
                source_ref,
                self.target_namespace,
            );
            let key = self.resolved_elements.get(flat_idx).copied().flatten();
            (name, namespace, key)
        } else {
            // `build_element_term` errors here; the report just says so.
            return ElementTermView {
                name: "(anonymous element without name or ref)".to_string(),
                declaration: DeclarationBinding::Unresolved,
                type_name: None,
                substitutable: Vec::new(),
            };
        };

        let resolved_type = if element_key.is_none() {
            self.resolved_types
                .get(flat_idx)
                .copied()
                .flatten()
                .or_else(|| match &elem.type_ref {
                    Some(TypeRefResult::QName(q)) => self
                        .schema_set
                        .lookup_type(q.namespace, q.local_name)
                        .or_else(|| {
                            self.schema_set
                                .get_built_in_type_by_qname(q.namespace, q.local_name)
                        }),
                    _ => None,
                })
        } else {
            None
        };

        element_view(
            self.schema_set,
            self.subst,
            name,
            namespace,
            element_key,
            resolved_type,
        )
    }
}

fn classify_occurs(min: u32, max: Option<u32>) -> OccurrenceCompilation {
    // Mirrors `apply_occurs`.
    match max {
        Some(1) if min == 1 => OccurrenceCompilation::Single,
        None if min > COUNTED_THRESHOLD => OccurrenceCompilation::Counted,
        None => OccurrenceCompilation::Unrolled,
        Some(m) if m <= COUNTED_THRESHOLD => OccurrenceCompilation::Unrolled,
        Some(_) => OccurrenceCompilation::Counted,
    }
}

fn authored_wildcard(schema_set: &SchemaSet, wildcard: &WildcardResult) -> WildcardTermView {
    let namespace = if wildcard.not_namespace.is_empty() {
        match &wildcard.namespace {
            WildcardNamespace::Any => "##any".to_string(),
            WildcardNamespace::Other => "##other".to_string(),
            WildcardNamespace::TargetNamespace => "##targetNamespace".to_string(),
            WildcardNamespace::Local => "##local".to_string(),
            WildcardNamespace::List(tokens) => namespace_token_list(schema_set, tokens),
        }
    } else {
        format!(
            "notNamespace={}",
            namespace_token_list(schema_set, &wildcard.not_namespace)
        )
    };
    let mut not_qnames: Vec<String> = wildcard
        .not_qname
        .iter()
        .map(|item| match item {
            NotQNameItem::QName {
                namespace,
                local_name,
            } => qname(schema_set, *namespace, *local_name),
            NotQNameItem::Defined => "##defined".to_string(),
            NotQNameItem::DefinedSibling => "##definedSibling".to_string(),
        })
        .collect();
    not_qnames.sort();
    WildcardTermView {
        namespace,
        process_contents: frame_process_contents(wildcard.process_contents),
        not_qnames,
    }
}

fn namespace_token_list(schema_set: &SchemaSet, tokens: &[NamespaceToken]) -> String {
    let mut names: Vec<String> = tokens
        .iter()
        .map(|t| match t {
            NamespaceToken::Uri(id) => schema_set.name_table.resolve(*id),
            NamespaceToken::Local => "##local".to_string(),
            NamespaceToken::TargetNamespace => "##targetNamespace".to_string(),
        })
        .collect();
    names.sort();
    format!("{{{}}}", names.join(" "))
}

fn frame_process_contents(pc: crate::parser::frames::ProcessContents) -> &'static str {
    use crate::parser::frames::ProcessContents as P;
    match pc {
        P::Strict => "strict",
        P::Lax => "lax",
        P::Skip => "skip",
    }
}

// ---------------------------------------------------------------------------
// Compiled view
// ---------------------------------------------------------------------------

fn compiled_view_from_matcher(
    schema_set: &SchemaSet,
    subst: Option<&SubstitutionGroupMap>,
    matcher: &ContentModelMatcher,
) -> CompiledView {
    match matcher {
        ContentModelMatcher::Nfa(nfa) => {
            CompiledView::Nfa(nfa_view(schema_set, subst, nfa, None, frontier_of(nfa)))
        }
        ContentModelMatcher::WithOpenContent {
            nfa,
            mode,
            wildcard,
        } => {
            let oc = open_content_from_wildcard(schema_set, *mode, wildcard.as_ref());
            CompiledView::Nfa(nfa_view(schema_set, subst, nfa, oc, frontier_of(nfa)))
        }
        ContentModelMatcher::AllGroup(model) => {
            CompiledView::AllGroup(all_group_view(schema_set, subst, model))
        }
    }
}

fn compiled_view_from_prepared(
    schema_set: &SchemaSet,
    subst: Option<&SubstitutionGroupMap>,
    model: &CompiledContentModel,
) -> CompiledView {
    match model {
        CompiledContentModel::Nfa { nfa, initial } => CompiledView::Nfa(nfa_view(
            schema_set,
            subst,
            nfa,
            None,
            frontier_from(initial),
        )),
        CompiledContentModel::NfaWithOpenContent {
            nfa,
            initial,
            open_content,
        } => {
            let oc = open_content.as_ref().map(|info| OpenContentView {
                mode: types_open_content_mode(info.mode),
                wildcard: Some(WildcardTermView {
                    namespace: namespace_constraint_name(schema_set, &info.namespace_constraint),
                    process_contents: process_contents_name(info.process_contents),
                    not_qnames: qname_pairs(schema_set, &info.not_qnames),
                }),
            });
            CompiledView::Nfa(nfa_view(schema_set, subst, nfa, oc, frontier_from(initial)))
        }
        CompiledContentModel::AllGroup(model) => {
            CompiledView::AllGroup(all_group_view(schema_set, subst, model))
        }
    }
}

fn nfa_view(
    schema_set: &SchemaSet,
    subst: Option<&SubstitutionGroupMap>,
    nfa: &NfaTable,
    open_content: Option<OpenContentView>,
    frontier: FrontierView,
) -> NfaView {
    let matcher = match &open_content {
        Some(oc) if oc.wildcard.is_some() => format!("NFA + open content ({})", oc.mode),
        _ => "NFA".to_string(),
    };
    let counters: Vec<CounterView> = nfa
        .counter_defs
        .iter()
        .enumerate()
        .map(|(i, def)| counter_view(i, def))
        .collect();
    let mut transition_count = 0;
    let mut states = Vec::with_capacity(nfa.states.len());
    let mut groups: Vec<SubstitutionGroupView> = Vec::new();
    for state in &nfa.states {
        transition_count += state.transitions.len();
        let term = state
            .term
            .as_ref()
            .map(|t| term_view(schema_set, subst, t, &mut groups));
        states.push(StateRow {
            id: state.id,
            start: state.id == nfa.start_state,
            accept: state.id == nfa.accept_state,
            term,
            transitions: state
                .transitions
                .iter()
                .map(|t| format_transition(t.kind, t.target))
                .collect(),
            origin: short_location(schema_set, state.origin.as_ref()),
        });
    }
    dedup_groups(&mut groups);
    let nullable_counters = counters
        .iter()
        .filter(|c| c.body_nullable)
        .map(|c| c.id)
        .collect();
    let exposure = ExposureView {
        counter_free: counters.is_empty(),
        counter_count: counters.len(),
        nullable_counters,
    };
    NfaView {
        matcher,
        state_count: nfa.states.len(),
        transition_count,
        counters,
        frontier,
        states,
        substitution_groups: groups,
        open_content,
        exposure,
        note: None,
    }
}

fn counter_view(index: usize, def: &CounterDef) -> CounterView {
    CounterView {
        id: index as CounterId,
        min: def.min,
        max: def.max,
        body_nullable: def.body_nullable,
    }
}

fn all_group_view(
    schema_set: &SchemaSet,
    subst: Option<&SubstitutionGroupMap>,
    model: &AllGroupModel,
) -> AllGroupView {
    let open_content = model.open_content.as_ref().map(|oc| OpenContentView {
        mode: all_group_open_content_mode(oc.mode),
        wildcard: Some(WildcardTermView {
            namespace: namespace_constraint_name(schema_set, &oc.namespace_constraint),
            process_contents: process_contents_name(oc.process_contents),
            not_qnames: qname_pairs(schema_set, &oc.not_qnames),
        }),
    });
    let matcher = match &open_content {
        Some(oc) if oc.wildcard.is_some() => format!("all-group + open content ({})", oc.mode),
        _ => "all-group".to_string(),
    };
    let mut groups: Vec<SubstitutionGroupView> = Vec::new();
    let mut members = Vec::with_capacity(model.particles.len());
    let mut required = Vec::new();
    for particle in &model.particles {
        let term = term_view(schema_set, subst, &particle.term, &mut groups);
        if particle.min_occurs > 0 {
            required.push(term.to_string());
        }
        members.push(AllMemberView {
            term,
            min_occurs: particle.min_occurs,
            max_occurs: particle.max_occurs.to_option(),
            origin: short_location(schema_set, particle.source.as_ref()),
        });
    }
    dedup_groups(&mut groups);
    AllGroupView {
        matcher,
        outer_optional: model.outer_optional,
        members,
        required,
        substitution_groups: groups,
        open_content,
        note: None,
    }
}

fn frontier_of(nfa: &NfaTable) -> FrontierView {
    match ActiveStates::try_from_nfa(nfa) {
        Ok(active) => frontier_from(&active),
        Err(limit) => FrontierView::LimitExceeded {
            message: limit.to_string(),
        },
    }
}

fn frontier_from(active: &ActiveStates) -> FrontierView {
    match active {
        ActiveStates::Simple(set) => FrontierView::Simple {
            states: set.iter().count(),
        },
        ActiveStates::Counted { configs, .. } => FrontierView::Counted {
            configs: configs.len(),
        },
        ActiveStates::RangedSingle { state_ranges, .. } => FrontierView::RangedSingle {
            states: state_ranges.len(),
        },
        ActiveStates::Hybrid {
            configs,
            ranged_counter_idx,
            ..
        } => FrontierView::Hybrid {
            configs: configs.len(),
            ranged_counter: *ranged_counter_idx,
        },
    }
}

fn format_transition(kind: TransitionKind, target: StateId) -> String {
    match kind {
        TransitionKind::Epsilon => format!("ε → #{target}"),
        TransitionKind::Consume => format!("consume → #{target}"),
        TransitionKind::CounterReset(c) => format!("reset c{c} → #{target}"),
        TransitionKind::CounterIncrement(c) => format!("c{c}++ → #{target}"),
        TransitionKind::CounterMaxGuard(c) => format!("c{c}<max → #{target}"),
        TransitionKind::CounterMinGuard(c) => format!("c{c}>=min → #{target}"),
    }
}

fn term_view(
    schema_set: &SchemaSet,
    subst: Option<&SubstitutionGroupMap>,
    term: &NfaTerm,
    groups: &mut Vec<SubstitutionGroupView>,
) -> TermView {
    match term {
        NfaTerm::Element {
            name,
            namespace,
            element_key,
            resolved_type,
        } => {
            let view = element_view(
                schema_set,
                subst,
                *name,
                *namespace,
                *element_key,
                *resolved_type,
            );
            if !view.substitutable.is_empty() {
                groups.push(SubstitutionGroupView {
                    head: view.name.clone(),
                    accepts: view.substitutable.clone(),
                });
            }
            TermView::Element(view)
        }
        NfaTerm::Wildcard {
            namespace_constraint,
            process_contents,
            not_qnames,
        } => TermView::Wildcard(WildcardTermView {
            namespace: namespace_constraint_name(schema_set, namespace_constraint),
            process_contents: process_contents_name(*process_contents),
            not_qnames: qname_pairs(schema_set, not_qnames),
        }),
    }
}

fn element_view(
    schema_set: &SchemaSet,
    subst: Option<&SubstitutionGroupMap>,
    name: NameId,
    namespace: Option<NameId>,
    element_key: Option<ElementKey>,
    resolved_type: Option<TypeKey>,
) -> ElementTermView {
    let declaration = match element_key {
        None => DeclarationBinding::Unresolved,
        Some(key) => {
            if schema_set.lookup_element(namespace, name) == Some(key) {
                DeclarationBinding::Global
            } else {
                DeclarationBinding::Local
            }
        }
    };
    let type_key = match element_key {
        Some(key) => schema_set
            .arenas
            .get_element(key)
            .and_then(|decl| decl.resolved_type),
        None => resolved_type,
    };
    let substitutable = match (element_key, subst) {
        (Some(key), Some(map)) => match map.get(&key) {
            Some(names) => {
                let mut list: Vec<String> = names
                    .iter()
                    .map(|(local, ns)| qname(schema_set, *ns, *local))
                    .collect();
                list.sort();
                list
            }
            None => Vec::new(),
        },
        _ => Vec::new(),
    };
    ElementTermView {
        name: qname(schema_set, namespace, name),
        declaration,
        type_name: type_key.map(|k| type_key_name(schema_set, k)),
        substitutable,
    }
}

fn dedup_groups(groups: &mut Vec<SubstitutionGroupView>) {
    groups.sort_by(|a, b| a.head.cmp(&b.head));
    groups.dedup_by(|a, b| a.head == b.head);
}

fn open_content_from_wildcard(
    schema_set: &SchemaSet,
    mode: TypesOpenContentMode,
    wildcard: Option<&WildcardRef>,
) -> Option<OpenContentView> {
    Some(OpenContentView {
        mode: types_open_content_mode(mode),
        wildcard: Some(WildcardTermView {
            namespace: namespace_constraint_name(schema_set, &wildcard?.namespace_constraint),
            process_contents: process_contents_name(wildcard?.process_contents),
            not_qnames: qname_pairs(schema_set, &wildcard?.not_qnames),
        }),
    })
}

fn types_open_content_mode(mode: TypesOpenContentMode) -> &'static str {
    match mode {
        TypesOpenContentMode::None => "none",
        TypesOpenContentMode::Interleave => "interleave",
        TypesOpenContentMode::Suffix => "suffix",
    }
}

fn all_group_open_content_mode(mode: AllGroupOpenContentMode) -> &'static str {
    match mode {
        AllGroupOpenContentMode::None => "none",
        AllGroupOpenContentMode::Interleave => "interleave",
        AllGroupOpenContentMode::Suffix => "suffix",
    }
}

fn process_contents_name(pc: ProcessContents) -> &'static str {
    match pc {
        ProcessContents::Strict => "strict",
        ProcessContents::Lax => "lax",
        ProcessContents::Skip => "skip",
    }
}

fn namespace_constraint_name(schema_set: &SchemaSet, nc: &NamespaceConstraint) -> String {
    match nc {
        NamespaceConstraint::Any => "##any".to_string(),
        NamespaceConstraint::Other => "##other".to_string(),
        NamespaceConstraint::TargetNamespace => "##targetNamespace".to_string(),
        NamespaceConstraint::Local => "##local".to_string(),
        NamespaceConstraint::List(list) => format!("{{{}}}", namespace_list(schema_set, list)),
        NamespaceConstraint::Not(list) => {
            format!("notNamespace={{{}}}", namespace_list(schema_set, list))
        }
    }
}

fn namespace_list(schema_set: &SchemaSet, list: &[Option<NameId>]) -> String {
    let mut names: Vec<String> = list
        .iter()
        .map(|ns| match ns {
            Some(id) => schema_set.name_table.resolve(*id),
            None => "##local".to_string(),
        })
        .collect();
    names.sort();
    names.join(" ")
}

fn qname_pairs(schema_set: &SchemaSet, pairs: &[(Option<NameId>, NameId)]) -> Vec<String> {
    let mut names: Vec<String> = pairs
        .iter()
        .map(|(ns, local)| qname(schema_set, *ns, *local))
        .collect();
    names.sort();
    names.dedup();
    names
}

// ---------------------------------------------------------------------------
// Name and location helpers
// ---------------------------------------------------------------------------

/// Format an expanded name as `{namespace}local`, abbreviating the XSD
/// namespace to the conventional `xs:` prefix.
fn qname(schema_set: &SchemaSet, namespace: Option<NameId>, local: NameId) -> String {
    let local = schema_set.name_table.resolve(local);
    match namespace {
        None => local,
        Some(ns) => {
            let ns = schema_set.name_table.resolve(ns);
            if ns.is_empty() {
                local
            } else if ns == XS_NAMESPACE {
                format!("xs:{local}")
            } else {
                format!("{{{ns}}}{local}")
            }
        }
    }
}

fn complex_type_name(schema_set: &SchemaSet, ct_data: &ComplexTypeDefData) -> String {
    match ct_data.name {
        Some(name) => qname(schema_set, ct_data.target_namespace, name),
        None => "(anonymous)".to_string(),
    }
}

fn type_key_name(schema_set: &SchemaSet, key: TypeKey) -> String {
    match key {
        TypeKey::Complex(k) => match schema_set.arenas.get_complex_type(k) {
            Some(data) => complex_type_name(schema_set, data),
            None => "(unknown complex type)".to_string(),
        },
        TypeKey::Simple(k) => match schema_set.arenas.get_simple_type(k) {
            Some(data) => match data.name {
                Some(name) => qname(schema_set, data.target_namespace, name),
                None => "(anonymous simple type)".to_string(),
            },
            None => "(unknown simple type)".to_string(),
        },
    }
}

/// `file.xsd:line:column` — the document's file name only, so state tables stay
/// narrow. The full base URI is in the Source view.
fn short_location(schema_set: &SchemaSet, source: Option<&SourceRef>) -> Option<String> {
    let loc = schema_set.locate(source)?;
    let file = loc.base_uri.rsplit('/').next().unwrap_or("");
    let file = if file.is_empty() {
        loc.base_uri.as_str()
    } else {
        file
    };
    Some(format!("{file}:{}:{}", loc.line, loc.column))
}

fn occurs(min: u32, max: Option<u32>) -> String {
    match max {
        Some(m) => format!("{min}..{m}"),
        None => format!("{min}..unbounded"),
    }
}

fn content_type_name(ct: ContentType) -> &'static str {
    match ct {
        ContentType::Empty => "empty",
        ContentType::TextOnly => "simple",
        ContentType::ElementOnly => "element-only",
        ContentType::Mixed => "mixed",
    }
}

fn xsd_version_name(v: XsdVersion) -> &'static str {
    match v {
        XsdVersion::V1_0 => "1.0",
        XsdVersion::V1_1 => "1.1",
    }
}

fn compositor_name(c: Compositor) -> &'static str {
    match c {
        Compositor::Sequence => "sequence",
        Compositor::Choice => "choice",
        Compositor::All => "all",
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

const KEY_WIDTH: usize = 15;

fn kv(f: &mut fmt::Formatter<'_>, key: &str, value: &str) -> fmt::Result {
    writeln!(f, "  {key:<KEY_WIDTH$}{value}")
}

fn heading(f: &mut fmt::Formatter<'_>, title: &str) -> fmt::Result {
    writeln!(f, "{title}")?;
    writeln!(f, "{}", "-".repeat(title.len()))
}

impl fmt::Display for ContentModelReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Content model report: {}", self.source.type_name)?;
        writeln!(f)?;
        self.source.fmt(f)?;
        writeln!(f)?;
        self.authored.fmt(f)?;
        writeln!(f)?;
        self.compiled.fmt(f)
    }
}

impl fmt::Display for SourceView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        heading(f, "Source")?;
        kv(f, "type", &self.type_name)?;
        kv(
            f,
            "document",
            self.document.as_deref().unwrap_or("(no source location)"),
        )?;
        match (self.line, self.column) {
            (Some(line), Some(column)) => kv(f, "at", &format!("line {line}, column {column}"))?,
            _ => kv(f, "at", "(unknown)")?,
        }
        kv(f, "content type", content_type_name(self.content_type))?;
        kv(f, "mixed", if self.mixed { "yes" } else { "no" })?;
        kv(
            f,
            "derivation",
            self.derivation.as_deref().unwrap_or("(none)"),
        )?;
        match &self.open_content {
            Some(oc) => kv(f, "open content", &oc.to_string())?,
            None => kv(f, "open content", "(none)")?,
        }
        kv(f, "xsd version", xsd_version_name(self.xsd_version))
    }
}

impl fmt::Display for AuthoredView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        heading(f, "Authored particles")?;
        if self.sections.is_empty() {
            return writeln!(f, "  (no content particle)");
        }
        let multi = self.sections.len() > 1;
        for section in &self.sections {
            if let Some(base) = &section.from_base_type {
                writeln!(f, "  from base type {base} (prepended by extension):")?;
            } else if multi {
                writeln!(f, "  own content:")?;
            }
            let depth = usize::from(multi);
            match &section.root {
                Some(root) => write_node(f, root, depth)?,
                None => writeln!(f, "  {}(no content particle)", "  ".repeat(depth))?,
            }
        }
        Ok(())
    }
}

fn write_node(f: &mut fmt::Formatter<'_>, node: &AuthoredNode, depth: usize) -> fmt::Result {
    let indent = "  ".repeat(depth + 1);
    let label = match &node.kind {
        AuthoredKind::Sequence => "sequence".to_string(),
        AuthoredKind::Choice => "choice".to_string(),
        AuthoredKind::All => "all".to_string(),
        AuthoredKind::GroupRef(g) => {
            let mut s = format!("group ref {}", g.name);
            if let Some(c) = g.compositor {
                s.push_str(&format!(" ({})", compositor_name(c)));
            }
            if !g.resolved {
                s.push_str(" [unresolved]");
            } else if g.recursive {
                s.push_str(" [recursive, not expanded]");
            }
            s
        }
        AuthoredKind::Element(e) => e.to_string(),
        AuthoredKind::Wildcard(w) => w.to_string(),
    };
    write!(
        f,
        "{indent}{label}  {}  [{}]",
        occurs(node.min_occurs, node.max_occurs),
        node.occurrence
    )?;
    match &node.origin {
        Some(origin) => writeln!(f, "  ({origin})")?,
        None => writeln!(f)?,
    }
    for child in &node.children {
        write_node(f, child, depth + 1)?;
    }
    Ok(())
}

impl fmt::Display for CompiledView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        heading(f, "Compiled")?;
        match self {
            CompiledView::Nfa(view) => view.fmt(f),
            CompiledView::AllGroup(view) => view.fmt(f),
            CompiledView::Failed { reason } => {
                kv(f, "matcher", "(none — preparation failed)")?;
                kv(f, "failure", reason)?;
                writeln!(
                    f,
                    "  the first element governed by this type raises \
                     validation-preparation-failed and aborts the run"
                )
            }
        }
    }
}

impl fmt::Display for NfaView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        kv(f, "matcher", &self.matcher)?;
        kv(f, "states", &self.state_count.to_string())?;
        kv(f, "transitions", &self.transition_count.to_string())?;
        if self.counters.is_empty() {
            kv(f, "counters", "(none)")?;
        } else {
            for c in &self.counters {
                let line = format!(
                    "c{}: {}..{}, body nullable? {}",
                    c.id,
                    c.min,
                    c.max,
                    if c.body_nullable { "yes" } else { "no" }
                );
                kv(f, if c.id == 0 { "counters" } else { "" }, &line)?;
            }
        }
        kv(f, "frontier", &self.frontier.to_string())?;
        if let Some(oc) = &self.open_content {
            kv(f, "open content", &oc.to_string())?;
        }
        kv(f, "exposure", &self.exposure.to_string())?;
        if let Some(note) = &self.note {
            kv(f, "note", note)?;
        }
        writeln!(f)?;
        write_state_table(f, &self.states)?;
        write_substitution_groups(f, &self.substitution_groups)
    }
}

fn write_state_table(f: &mut fmt::Formatter<'_>, states: &[StateRow]) -> fmt::Result {
    let rows: Vec<(String, String, String, String, &str)> = states
        .iter()
        .map(|s| {
            let flags = match (s.start, s.accept) {
                (true, true) => "start,accept",
                (true, false) => "start",
                (false, true) => "accept",
                (false, false) => "",
            };
            (
                format!("#{}", s.id),
                flags.to_string(),
                s.term
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "(epsilon)".to_string()),
                if s.transitions.is_empty() {
                    "(none)".to_string()
                } else {
                    s.transitions.join(", ")
                },
                s.origin.as_deref().unwrap_or("(no origin)"),
            )
        })
        .collect();
    let w_id = rows.iter().map(|r| r.0.len()).chain([3]).max().unwrap_or(3);
    let w_flags = rows.iter().map(|r| r.1.len()).chain([5]).max().unwrap_or(5);
    let w_term = rows.iter().map(|r| r.2.len()).chain([4]).max().unwrap_or(4);
    let w_trans = rows
        .iter()
        .map(|r| r.3.len())
        .chain([11])
        .max()
        .unwrap_or(11);
    writeln!(
        f,
        "  {:<w_id$}  {:<w_flags$}  {:<w_term$}  {:<w_trans$}  origin",
        "#id", "flags", "term", "transitions"
    )?;
    writeln!(
        f,
        "  {}  {}  {}  {}  {}",
        "-".repeat(w_id),
        "-".repeat(w_flags),
        "-".repeat(w_term),
        "-".repeat(w_trans),
        "-".repeat(6)
    )?;
    for (id, flags, term, trans, origin) in &rows {
        writeln!(
            f,
            "  {id:<w_id$}  {flags:<w_flags$}  {term:<w_term$}  {trans:<w_trans$}  {origin}"
        )?;
    }
    Ok(())
}

fn write_substitution_groups(
    f: &mut fmt::Formatter<'_>,
    groups: &[SubstitutionGroupView],
) -> fmt::Result {
    if groups.is_empty() {
        return Ok(());
    }
    writeln!(f)?;
    writeln!(f, "  substitution groups")?;
    for g in groups {
        writeln!(f, "    {} accepts {}", g.head, g.accepts.join(", "))?;
    }
    Ok(())
}

impl fmt::Display for AllGroupView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        kv(f, "matcher", &self.matcher)?;
        kv(
            f,
            "outer",
            if self.outer_optional {
                "optional (minOccurs=0)"
            } else {
                "required (minOccurs=1)"
            },
        )?;
        kv(f, "members", &self.members.len().to_string())?;
        if let Some(oc) = &self.open_content {
            kv(f, "open content", &oc.to_string())?;
        }
        if let Some(note) = &self.note {
            kv(f, "note", note)?;
        }
        writeln!(f)?;
        for m in &self.members {
            writeln!(
                f,
                "    {}  {}  ({})",
                m.term,
                occurs(m.min_occurs, m.max_occurs),
                m.origin.as_deref().unwrap_or("no origin")
            )?;
        }
        writeln!(f)?;
        if self.required.is_empty() {
            writeln!(f, "  required members: (none — the group may stay empty)")?;
        } else {
            writeln!(f, "  required members:")?;
            for r in &self.required {
                writeln!(f, "    {r}")?;
            }
        }
        write_substitution_groups(f, &self.substitution_groups)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SchemaSetBuilder;
    use crate::validation::{SchemaValidator, ValidationFlags};

    fn schema(xsd: &str) -> SchemaSet {
        SchemaSetBuilder::new()
            .add_source(xsd, "file:///inspect.xsd")
            .expect("schema source accepted")
            .compile()
            .expect("schema compiles")
            .into_schema_set()
    }

    #[cfg(feature = "xsd11")]
    fn schema11(xsd: &str) -> SchemaSet {
        SchemaSetBuilder::xsd11()
            .add_source(xsd, "file:///inspect11.xsd")
            .expect("schema source accepted")
            .compile()
            .expect("schema compiles")
            .into_schema_set()
    }

    fn report(schema_set: &SchemaSet, local: &str) -> String {
        let key = find_complex_type(schema_set, None, local)
            .unwrap_or_else(|| panic!("no named complex type {local}"));
        inspect_content_model(schema_set, key)
            .expect("content model compiles")
            .to_string()
    }

    /// The anonymous complex type of a named global element declaration.
    fn anonymous_type_of(schema_set: &SchemaSet, element: &str) -> ComplexTypeKey {
        let name = schema_set
            .name_table
            .get(element)
            .expect("element interned");
        let key = schema_set
            .lookup_element(None, name)
            .expect("global element");
        match schema_set.arenas.elements[key].resolved_type {
            Some(TypeKey::Complex(k)) => k,
            other => panic!("element {element} has no complex type: {other:?}"),
        }
    }

    #[test]
    fn plain_sequence_shows_three_views() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Pair">
    <xs:sequence>
      <xs:element name="a" type="xs:string"/>
      <xs:element name="b" type="xs:int" minOccurs="0"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Pair");

        // Three labelled views.
        assert!(text.contains("Source"), "{text}");
        assert!(text.contains("Authored particles"), "{text}");
        assert!(text.contains("Compiled"), "{text}");

        // Source view.
        assert!(text.contains("type           Pair"), "{text}");
        assert!(text.contains("file:///inspect.xsd"), "{text}");
        assert!(text.contains("content type   element-only"), "{text}");
        assert!(text.contains("mixed          no"), "{text}");
        assert!(text.contains("xsd version    1.0"), "{text}");

        // Authored view: tree, bounds, unroll decision, locations.
        assert!(text.contains("sequence  1..1  [single]"), "{text}");
        assert!(
            text.contains("element a : xs:string [local decl]"),
            "{text}"
        );
        assert!(text.contains("element b : xs:int [local decl]"), "{text}");
        assert!(text.contains("0..1  [unrolled]"), "{text}");
        assert!(text.contains("inspect.xsd:"), "{text}");

        // Compiled view.
        assert!(text.contains("matcher        NFA"), "{text}");
        assert!(text.contains("counters       (none)"), "{text}");
        assert!(text.contains("frontier       Simple("), "{text}");
        assert!(text.contains("counter-free"), "{text}");
        assert!(text.contains("consume → #"), "{text}");
        assert!(text.contains("start"), "{text}");
        assert!(text.contains("accept"), "{text}");
    }

    #[test]
    fn unbounded_choice_is_unrolled_with_no_counter() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Stream">
    <xs:choice minOccurs="0" maxOccurs="unbounded">
      <xs:element name="a" type="xs:string"/>
      <xs:element name="b" type="xs:string"/>
    </xs:choice>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Stream");
        assert!(text.contains("choice  0..unbounded  [unrolled]"), "{text}");
        assert!(text.contains("counters       (none)"), "{text}");
        assert!(text.contains("counter-free"), "{text}");
        assert!(text.contains("frontier       Simple("), "{text}");
    }

    #[test]
    fn large_bound_compiles_to_a_counter_not_an_unrolled_range() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Big">
    <xs:sequence>
      <xs:element name="a" type="xs:string" minOccurs="0" maxOccurs="10001"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Big");
        assert!(text.contains("0..10001  [counter]"), "{text}");
        assert!(text.contains("c0: 0..10001"), "{text}");
        assert!(text.contains("body nullable? no"), "{text}");
        assert!(!text.contains("counter-free"), "{text}");
        // Counter transitions are spelled out in the state table.
        assert!(text.contains("reset c0 → #"), "{text}");
        assert!(text.contains("c0++ → #"), "{text}");
        assert!(text.contains("c0<max → #"), "{text}");
        assert!(text.contains("c0>=min → #"), "{text}");
        // A counted, non-nullable body still uses the scalar path.
        assert!(text.contains("frontier       Counted("), "{text}");
    }

    #[test]
    fn nullable_counted_body_is_flagged_as_exposed() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Nullable">
    <xs:sequence minOccurs="0" maxOccurs="500">
      <xs:element name="a" type="xs:string" minOccurs="0"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Nullable");
        assert!(text.contains("body nullable? yes"), "{text}");
        assert!(text.contains("nullable body in c0"), "{text}");
        assert!(text.contains("MAX_ACTIVE_CONFIGS"), "{text}");
        assert!(text.contains("frontier       RangedSingle("), "{text}");
    }

    #[test]
    fn all_group_lists_members_and_required_members() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Unordered">
    <xs:all>
      <xs:element name="a" type="xs:string"/>
      <xs:element name="b" type="xs:string" minOccurs="0"/>
    </xs:all>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Unordered");
        assert!(text.contains("matcher        all-group"), "{text}");
        assert!(text.contains("outer          required"), "{text}");
        assert!(text.contains("members        2"), "{text}");
        assert!(text.contains("required members:"), "{text}");
        assert!(text.contains("element a"), "{text}");
        // All-group bounds are enforced by the group's own counters.
        assert!(text.contains("[all-group counter]"), "{text}");
        assert!(text.contains("all  1..1"), "{text}");
    }

    #[test]
    fn substitution_group_head_lists_its_members() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="shape" type="xs:string"/>
  <xs:element name="circle" type="xs:string" substitutionGroup="shape"/>
  <xs:element name="square" type="xs:string" substitutionGroup="shape"/>
  <xs:complexType name="Canvas">
    <xs:sequence>
      <xs:element ref="shape" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Canvas");
        assert!(text.contains("element shape"), "{text}");
        assert!(text.contains("[global decl"), "{text}");
        assert!(text.contains("+2 substitutable"), "{text}");
        assert!(text.contains("substitution groups"), "{text}");
        assert!(
            text.contains("shape accepts circle, shape, square"),
            "{text}"
        );
    }

    #[test]
    fn anonymous_local_type_is_named_anonymous() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="root">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="a" type="xs:string"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#,
        );
        let key = anonymous_type_of(&ss, "root");
        let text = inspect_content_model(&ss, key)
            .expect("model compiles")
            .to_string();
        assert!(text.contains("Content model report: (anonymous)"), "{text}");
        assert!(text.contains("type           (anonymous)"), "{text}");
        assert!(text.contains("element a : xs:string"), "{text}");
        // A named type can not be found for it.
        assert!(find_complex_type(&ss, None, "root").is_none());
    }

    #[test]
    fn group_reference_is_expanded_but_labelled_as_a_reference() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:group name="names">
    <xs:sequence>
      <xs:element name="first" type="xs:string"/>
      <xs:element name="last" type="xs:string"/>
    </xs:sequence>
  </xs:group>
  <xs:complexType name="Person">
    <xs:sequence>
      <xs:group ref="names"/>
      <xs:element name="age" type="xs:int"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Person");
        assert!(text.contains("group ref names (sequence)"), "{text}");
        assert!(text.contains("element first : xs:string"), "{text}");
        assert!(text.contains("element last : xs:string"), "{text}");
        assert!(text.contains("element age : xs:int"), "{text}");
    }

    #[test]
    fn extension_shows_the_base_contribution_first() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Base">
    <xs:sequence>
      <xs:element name="a" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>
  <xs:complexType name="Derived">
    <xs:complexContent>
      <xs:extension base="Base">
        <xs:sequence>
          <xs:element name="b" type="xs:string"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
</xs:schema>"#,
        );
        let text = report(&ss, "Derived");
        assert!(text.contains("derivation     extension of Base"), "{text}");
        let base_at = text.find("from base type Base").expect("base section");
        let own_at = text.find("own content:").expect("own section");
        assert!(base_at < own_at, "base section comes first:\n{text}");
        assert!(text.contains("element a : xs:string"), "{text}");
        assert!(text.contains("element b : xs:string"), "{text}");
    }

    #[test]
    fn wildcard_shows_constraint_and_process_contents() {
        let ss = schema(
            r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
             targetNamespace="urn:w" xmlns:w="urn:w">
  <xs:complexType name="Open">
    <xs:sequence>
      <xs:any namespace="##other" processContents="lax" minOccurs="0"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"###,
        );
        let key = find_complex_type(&ss, Some("urn:w"), "Open").expect("named type");
        let text = inspect_content_model(&ss, key)
            .expect("model compiles")
            .to_string();
        assert!(text.contains("any ##other [processContents=lax]"), "{text}");
        assert!(text.contains("{urn:w}Open"), "{text}");
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn open_content_is_reported_in_source_and_compiled_views() {
        let ss = schema11(
            r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Extensible">
    <xs:openContent mode="interleave">
      <xs:any namespace="##other" processContents="lax"/>
    </xs:openContent>
    <xs:sequence>
      <xs:element name="a" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"###,
        );
        let text = report(&ss, "Extensible");
        assert!(
            text.contains("matcher        NFA + open content (interleave)"),
            "{text}"
        );
        assert!(
            text.contains("open content   interleave, any ##other"),
            "{text}"
        );
        assert!(text.contains("xsd version    1.1"), "{text}");
    }

    #[cfg(feature = "xsd11")]
    #[test]
    fn all_group_open_content_suffix_is_reported() {
        let ss = schema11(
            r###"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="OpenAll">
    <xs:openContent mode="suffix">
      <xs:any namespace="##other" processContents="skip"/>
    </xs:openContent>
    <xs:all>
      <xs:element name="a" type="xs:string"/>
      <xs:element name="b" type="xs:string" minOccurs="0"/>
    </xs:all>
  </xs:complexType>
</xs:schema>"###,
        );
        let text = report(&ss, "OpenAll");
        assert!(
            text.contains("matcher        all-group + open content (suffix)"),
            "{text}"
        );
        assert!(text.contains("processContents=skip"), "{text}");
    }

    /// `describe_content_model` must reuse the validator's prepared model.
    #[test]
    fn describe_content_model_uses_the_prepared_model() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Pair">
    <xs:sequence>
      <xs:element name="a" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let key = find_complex_type(&ss, None, "Pair").expect("named type");
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let text = validator
            .describe_content_model(key)
            .expect("type is in the schema set")
            .to_string();
        assert!(text.contains("matcher        NFA"), "{text}");
        assert!(text.contains("element a : xs:string"), "{text}");
        // The free function compiles the same thing.
        let fresh = inspect_content_model(&ss, key)
            .expect("compiles")
            .to_string();
        assert_eq!(text, fresh, "prepared and freshly compiled views agree");
    }

    /// `((a?){0,1000000}){0,1000000}`: preparation fails, and the report says so.
    #[test]
    fn describe_content_model_names_a_preparation_failure() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r">
    <xs:complexType>
      <xs:sequence>
        <xs:sequence minOccurs="0" maxOccurs="1000000">
          <xs:sequence minOccurs="0" maxOccurs="1000000">
            <xs:element name="a" type="xs:string" minOccurs="0"/>
          </xs:sequence>
        </xs:sequence>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#,
        );
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let failures = validator.content_model_failures();
        assert_eq!(failures.len(), 1, "exactly one failure: {failures:?}");
        let key = failures[0].0;

        let text = validator
            .describe_content_model(key)
            .expect("type is in the schema set")
            .to_string();
        assert!(text.contains("(none — preparation failed)"), "{text}");
        assert!(text.contains("execution limit exceeded"), "{text}");
        assert!(text.contains("validation-preparation-failed"), "{text}");
        // The authored view still renders, so the shape that failed is visible.
        assert!(text.contains("0..1000000  [counter]"), "{text}");

        // The free function still compiles the model — only preparation failed.
        let fresh = inspect_content_model(&ss, key).expect("the model compiles");
        assert!(matches!(fresh.compiled, CompiledView::Nfa(_)));
        assert!(
            matches!(
                &fresh.compiled,
                CompiledView::Nfa(v) if matches!(v.frontier, FrontierView::LimitExceeded { .. })
            ),
            "the frontier names the limit: {}",
            fresh
        );
    }

    /// No model is prepared for an empty-content type, so the view is
    /// compiled on demand and says so.
    #[test]
    fn describe_content_model_marks_an_unprepared_model() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Marker">
    <xs:attribute name="id" type="xs:string"/>
  </xs:complexType>
</xs:schema>"#,
        );
        let key = find_complex_type(&ss, None, "Marker").expect("named type");
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        let text = validator
            .describe_content_model(key)
            .expect("type is in the schema set")
            .to_string();
        assert!(text.contains("content type   empty"), "{text}");
        assert!(text.contains("compiled on demand"), "{text}");
        assert!(text.contains("(no content particle)"), "{text}");
    }

    #[test]
    fn describe_content_model_returns_none_for_a_foreign_key() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Pair"><xs:sequence/></xs:complexType>
</xs:schema>"#,
        );
        let other = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Other">
    <xs:sequence><xs:element name="z" type="xs:string"/></xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        // A key from a *different* schema set is (almost certainly) absent here;
        // slotmap keys carry a version, so the lookup must not panic.
        let foreign = find_complex_type(&other, None, "Other").expect("named type");
        let validator = SchemaValidator::new(&ss, ValidationFlags::default());
        // Either None (key absent) or Some (key collides with a live slot) is
        // acceptable — the contract is only that it does not panic.
        let _ = validator.describe_content_model(foreign);
    }

    #[test]
    fn find_complex_type_rejects_simple_types_and_unknown_names() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:simpleType name="Code">
    <xs:restriction base="xs:string"/>
  </xs:simpleType>
  <xs:complexType name="Pair"><xs:sequence/></xs:complexType>
</xs:schema>"#,
        );
        assert!(find_complex_type(&ss, None, "Code").is_none());
        assert!(find_complex_type(&ss, None, "Nope").is_none());
        assert!(find_complex_type(&ss, Some("urn:nope"), "Pair").is_none());
        assert!(find_complex_type(&ss, None, "Pair").is_some());
    }

    /// Branch, merge and occurrence-wrapper states are invented by the
    /// compiler rather than written by the schema author, but each still
    /// belongs to the construct that caused it. Every state row must name a
    /// location — the state table used to render about half its rows as
    /// `(no origin)`.
    #[test]
    fn epsilon_states_carry_the_origin_of_their_construct() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:complexType name="Mixed">
    <xs:sequence>
      <xs:element name="a" type="xs:string"/>
      <xs:choice minOccurs="0" maxOccurs="3">
        <xs:element name="b" type="xs:string"/>
        <xs:element name="c" type="xs:string"/>
      </xs:choice>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#,
        );
        let key = find_complex_type(&ss, None, "Mixed").expect("named type");
        let rep = inspect_content_model(&ss, key).expect("content model compiles");
        let CompiledView::Nfa(view) = &rep.compiled else {
            panic!("expected an NFA model, got {}", rep.compiled.matcher_kind());
        };

        let epsilons: Vec<&StateRow> = view.states.iter().filter(|s| s.term.is_none()).collect();
        assert!(
            !epsilons.is_empty(),
            "this model is built by composition, so it has epsilon states"
        );
        for state in &epsilons {
            assert!(
                state.origin.is_some(),
                "epsilon state #{} has no origin",
                state.id
            );
        }

        let text = rep.to_string();
        assert!(!text.contains("(no origin)"), "{text}");
    }

    /// Report rendering must not depend on hash-map iteration order.
    #[test]
    fn rendering_is_deterministic() {
        let ss = schema(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="shape" type="xs:string"/>
  <xs:element name="circle" type="xs:string" substitutionGroup="shape"/>
  <xs:element name="square" type="xs:string" substitutionGroup="shape"/>
  <xs:complexType name="Canvas">
    <xs:choice maxOccurs="unbounded">
      <xs:element ref="shape"/>
      <xs:any namespace="urn:b urn:a urn:c" processContents="skip"/>
    </xs:choice>
  </xs:complexType>
</xs:schema>"#,
        );
        let first = report(&ss, "Canvas");
        for _ in 0..4 {
            assert_eq!(first, report(&ss, "Canvas"));
        }
        assert!(first.contains("{urn:a urn:b urn:c}"), "{first}");
    }
}
