//! NFA compilation for XSD content models
//!
//! This module compiles XSD content model particles (sequences, choices, all-groups)
//! into NFAs that can be used for efficient content validation.
//!
//! # Architecture
//!
//! The compiler uses Thompson's construction with composable fragments:
//! - Each element or wildcard becomes a single-state fragment
//! - Sequences concatenate fragments with epsilon transitions
//! - Choices add new start/end states with epsilon branches
//! - Repetition adds epsilon loops based on occurrence constraints
//!
//! # Example
//!
//! ```
//! use xsd_schema::compiler::{CompileContext, FragmentBuilder, NfaTerm, fragment_to_table};
//! use xsd_schema::{SchemaSet, NameId};
//!
//! // Build a simple NFA for a single element
//! let builder = FragmentBuilder::new();
//! let term = NfaTerm::element(NameId(1), None, None);
//! let fragment = builder.single_term(term, None);
//! let nfa = fragment_to_table(fragment);
//!
//! assert_eq!(nfa.state_count(), 2); // term state + exit state
//! ```

mod all_group;
mod compile;
mod error;
mod fragment;
pub mod inspect;
mod nfa;
#[cfg(feature = "xsd11")]
mod open_content;
mod particle;
pub(crate) mod substitution;
mod upa;

pub use all_group::{
    term_matches, term_matches_with_substitution, validate_all_group_constraints, AllGroupModel,
    AllGroupState, AllParticle, OpenContentMode, OpenContentWildcard, TermMatchResult,
};
pub use compile::{
    compile_content_model_for_upa, compile_content_model_matcher, compile_model_group,
    compile_particle, CompileContext,
};
pub(crate) use compile::{
    is_top_level_all_group, resolve_top_level_all_group_ref, validate_outer_all_group_occurs,
};
pub use error::{NfaCompileError, NfaCompileResult};
pub use fragment::{fragment_to_table, FragmentBuilder, NfaFragment};
pub use inspect::{
    find_complex_type, inspect_content_model, AllGroupView, AllMemberView, AuthoredKind,
    AuthoredNode, AuthoredSection, AuthoredView, CompiledView, ContentModelReport, CounterView,
    DeclarationBinding, ElementTermView, ExposureView, FrontierView, GroupRefView, NfaView,
    OccurrenceCompilation, OpenContentView, SourceView, StateRow, SubstitutionGroupView, TermView,
    WildcardTermView,
};
pub use nfa::{
    advance_states, advance_with_priority, epsilon_closure, term_matches as nfa_term_matches,
    ActiveConfig, ActiveStates, ContentModelLimit, ContentModelLimitExceeded, CounterDef,
    CounterId, MatchInfo, NfaState, NfaTable, NfaTerm, NfaTransition, StateId, StateSet,
    TransitionKind, MAX_ACTIVE_CONFIGS, MAX_CLOSURE_WORK,
};
pub use particle::{apply_occurs, MaxOccurs};
pub use substitution::{
    build_substitution_group_map, build_substitution_group_map_with_abstract,
    validate_all_substitution_groups, SubstitutionGroupMap,
};
pub use upa::{check_all_group_upa, check_upa};
pub(crate) use upa::{
    check_all_group_upa_with_substitutions, check_upa_with_substitutions, upa_substitution_map,
};

use crate::types::complex::{OpenContentMode as TypesOpenContentMode, WildcardRef};

/// Strategy for matching compiled content models.
#[derive(Debug, Clone)]
pub enum ContentModelMatcher {
    /// Standard NFA-based content model.
    Nfa(NfaTable),
    /// All-group content model.
    AllGroup(AllGroupModel),
    /// NFA content model with open content wildcard.
    WithOpenContent {
        nfa: NfaTable,
        mode: TypesOpenContentMode,
        wildcard: Option<WildcardRef>,
    },
}

#[cfg(feature = "xsd11")]
pub use open_content::validate_all_default_open_content;
