//! Compile-time dependency analysis for compiled XPath expressions.
//!
//! After [`bind_node`](crate::xpath::bind::bind_node) has resolved variables,
//! functions and name tests, [`analyze`] walks the bound AST once and records
//! what the expression actually *depends on*:
//!
//! - which external variable slots are referenced (a bitset over the slots
//!   allocated before `NameBinder::mark_external_boundary()`),
//! - whether evaluation reads the **initial focus** — the context item,
//!   `fn:position()` or `fn:last()` of the dynamic context that is in effect
//!   when the expression is entered,
//! - the distinct `(namespace, local name, arity)` triples of every function
//!   called anywhere in the expression.
//!
//! The analysis runs once per compilation and adds nothing to the evaluation
//! path; the result is stored in [`XPathExpr`](crate::xpath::api::XPathExpr)
//! and exposed through its accessors.
//!
//! # The "initial focus" rule
//!
//! XPath 2.0 §2.1.2 defines the focus and names the only constructs that
//! replace it: "Certain language constructs, notably the path expression
//! `E1/E2` and the predicate `E1[E2]`, create a new focus for the evaluation
//! of a sub-expression. In these constructs, `E2` is evaluated once for each
//! item in the sequence that results from evaluating `E1`."
//!
//! So the walk carries a boolean "still in the initial focus" and clears it
//! when it descends into
//!
//! - any predicate (§3.2.2: "the predicate expression is evaluated using an
//!   inner focus" whose context item "is the item currently being tested
//!   against the predicate"), including the predicates of a filter expression
//!   (§3.3.2),
//! - every path step but the first of a relative path (§3.2: "Each node
//!   resulting from the evaluation of `E1` then serves in turn to provide an
//!   inner focus for an evaluation of `E2`"),
//! - every step of an absolute path — `/` abbreviates
//!   `(fn:root(self::node()) treat as document-node())/` and `//` abbreviates
//!   `(fn:root(self::node()) treat as document-node())/descendant-or-self::node()/`
//!   (§3.2), so an absolute path *does* read the initial context node (to find
//!   its root) but its steps run in inner foci.
//!
//! Everything else keeps the enclosing focus: `for` (§3.7) and quantified
//! (§3.9) expressions only bind variables, and `if`, function arguments,
//! operators, ranges, type expressions and comma sequences do not mention the
//! focus at all.

use crate::xpath::arena::{AstArena, AstNodeId};
use crate::xpath::ast::{AstNode, FunctionCallNode};
use crate::xpath::context::{VarSlotId, XPathContext};
use crate::xpath::functions::extensible::handle_to_function_id;
use crate::xpath::functions::{FunctionId, FUNCTION_REGISTRY};

// ============================================================================
// FunctionCallRef
// ============================================================================

/// A distinct function call site found in a compiled XPath expression.
///
/// Produced by [`XPathExpr::function_calls`](crate::xpath::api::XPathExpr::function_calls),
/// which reports one entry per distinct `(namespace, local_name, arity)` triple
/// — including calls that appear inside predicates, `for` bodies and other
/// nested foci. Callers use it to detect functions whose results are not a
/// function of the focus and the arguments alone (`fn:doc`, `fn:collection`,
/// `fn:unparsed-text`, a host language's `key()` …) without this crate having to take a
/// position on what "impure" means.
///
/// `namespace` is the namespace the call actually **bound** to — the namespace
/// of the resolved function's signature, not the way the call was spelled. So
/// `count(...)` is reported in `http://www.w3.org/2005/xpath-functions` in both
/// XPath 2.0 and XPath 1.0 mode, even though 1.0 mode looks core functions up
/// without a namespace (`XPath10Catalog` resolves them through `FN_NAMESPACE`),
/// and a call written against the `FN_2010_NAMESPACE` alias is reported under
/// the canonical `fn:` namespace it resolves to. A test like
/// `namespace == FN_NAMESPACE && local_name == "doc"` is therefore mode
/// independent.
///
/// Only when no signature is reachable for the call — an unbound tree, or a
/// catalog that does not describe its own handle — does this fall back to the
/// lexical resolution of the prefix (or the static context's default function
/// namespace for an unprefixed call).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionCallRef {
    /// Namespace URI the call was bound in (may be empty).
    pub namespace: String,
    /// Local name of the called function.
    pub local_name: String,
    /// Number of arguments at the call site.
    pub arity: usize,
}

// ============================================================================
// ExprDependencies
// ============================================================================

/// Dependency metadata collected for one compiled expression.
///
/// Crate-internal: the public surface is the set of accessors on
/// [`XPathExpr`](crate::xpath::api::XPathExpr).
#[derive(Debug, Clone, Default)]
pub(crate) struct ExprDependencies {
    /// Bitset over external variable slots (slot `n` is bit `n % 64` of word
    /// `n / 64`). Only slots below the external boundary are recorded.
    referenced_vars: Vec<u64>,
    /// The distinct slots **at or above** the external boundary that the
    /// expression references, in first-occurrence order. For a whole compiled
    /// expression these are its own `for`/`some`/`every` variables; analysing a
    /// subtree with a boundary of `0` turns this into "every variable slot the
    /// subtree reads".
    inner_var_refs: Vec<VarSlotId>,
    uses_focus: bool,
    uses_position: bool,
    uses_last: bool,
    function_calls: Vec<FunctionCallRef>,
}

impl ExprDependencies {
    /// Whether evaluation reads the initial context item.
    #[inline]
    pub(crate) fn uses_focus(&self) -> bool {
        self.uses_focus
    }

    /// Whether `fn:position()` is evaluated against the initial focus.
    #[inline]
    pub(crate) fn uses_position(&self) -> bool {
        self.uses_position
    }

    /// Whether `fn:last()` is evaluated against the initial focus.
    #[inline]
    pub(crate) fn uses_last(&self) -> bool {
        self.uses_last
    }

    /// The distinct function calls of the expression.
    #[inline]
    pub(crate) fn function_calls(&self) -> &[FunctionCallRef] {
        &self.function_calls
    }

    /// Whether the external variable in `slot` is referenced. O(1).
    #[inline]
    pub(crate) fn references_var(&self, slot: VarSlotId) -> bool {
        let word = (slot / 64) as usize;
        match self.referenced_vars.get(word) {
            Some(bits) => bits & (1u64 << (slot % 64)) != 0,
            None => false,
        }
    }
}

// ============================================================================
// analyze
// ============================================================================

/// Analyze a **bound** expression tree rooted at `root`.
///
/// `external_slot_count` is the number of external variable slots, which
/// `XPathExpr::compile_with_vars` allocates first (slots `0 .. count`) and
/// closes with `NameBinder::mark_external_boundary()`; every variable the
/// expression introduces itself (`for`, `some`, `every`) gets a fresh slot at
/// or above that boundary, so a shadowing binding can never be mistaken for
/// the external of the same name.
///
/// Must be called after binding: unbound `VarRef`/`FunctionCall` nodes carry no
/// slot or handle, and the analysis then falls back to its conservative answer
/// (see `call_focus_use`).
pub(crate) fn analyze(
    arena: &AstArena,
    root: AstNodeId,
    ctx: &XPathContext<'_>,
    external_slot_count: usize,
) -> ExprDependencies {
    let mut analyzer = Analyzer {
        arena,
        ctx,
        external_slot_count,
        deps: ExprDependencies {
            referenced_vars: vec![0u64; external_slot_count.div_ceil(64)],
            ..Default::default()
        },
    };
    analyzer.walk(root, true);
    analyzer.deps
}

/// Which components of the focus a construct reads.
#[derive(Debug, Clone, Copy, Default)]
struct FocusUse {
    /// Reads the context item (or context node).
    item: bool,
    /// Reads the context position (`fn:position()`).
    position: bool,
    /// Reads the context size (`fn:last()`).
    size: bool,
}

impl FocusUse {
    const NONE: Self = Self {
        item: false,
        position: false,
        size: false,
    };
    const ITEM: Self = Self {
        item: true,
        position: false,
        size: false,
    };
    const POSITION: Self = Self {
        item: true,
        position: true,
        size: false,
    };
    const SIZE: Self = Self {
        item: true,
        position: false,
        size: true,
    };
}

struct Analyzer<'a, 'ctx> {
    arena: &'a AstArena,
    ctx: &'a XPathContext<'ctx>,
    external_slot_count: usize,
    deps: ExprDependencies,
}

impl Analyzer<'_, '_> {
    /// Walk one node. `in_initial_focus` is true while the node is evaluated
    /// with the dynamic context the whole expression was entered with.
    fn walk(&mut self, id: AstNodeId, in_initial_focus: bool) {
        // Copy the shared arena reference out of `self` so that the borrow of
        // the node does not conflict with the `&mut self` recursion below.
        let arena = self.arena;
        let Some(node) = arena.try_get(id) else {
            return;
        };

        match node {
            // Comma sequences keep the enclosing focus.
            AstNode::Expr(expr) => {
                for &item_id in &expr.items {
                    self.walk(item_id, in_initial_focus);
                }
            }

            AstNode::Value(_) => {}

            // `.` is the context item (§2.1.2: "The context item is returned
            // by an expression consisting of a single dot").
            AstNode::ContextItem(_) => self.note_focus(FocusUse::ITEM, in_initial_focus),

            AstNode::VarRef(var_ref) => {
                if let Some(slot) = var_ref.slot {
                    self.mark_var(slot);
                }
            }

            // `if` does not change the focus.
            AstNode::If(if_node) => {
                self.walk(if_node.test, in_initial_focus);
                self.walk(if_node.then_branch, in_initial_focus);
                self.walk(if_node.else_branch, in_initial_focus);
            }

            // §3.7: a `for` expression binds a range variable and evaluates the
            // return expression once per item of the binding sequence — the
            // focus is untouched in both.
            AstNode::For(for_node) => {
                for binding in &for_node.bindings {
                    self.walk(binding.in_expr, in_initial_focus);
                }
                self.walk(for_node.return_expr, in_initial_focus);
            }

            // §3.9: likewise for `some` / `every`.
            AstNode::Quantified(quant) => {
                for binding in &quant.bindings {
                    self.walk(binding.in_expr, in_initial_focus);
                }
                self.walk(quant.satisfies, in_initial_focus);
            }

            AstNode::FunctionCall(call) => {
                self.record_call(call);
                self.note_focus(call_focus_use(call), in_initial_focus);
                // Arguments are evaluated in the enclosing focus.
                for &arg_id in &call.args {
                    self.walk(arg_id, in_initial_focus);
                }
            }

            AstNode::PathExpr(path) => {
                if path.is_absolute {
                    // §3.2: a leading `/` or `//` abbreviates
                    // `(fn:root(self::node()) treat as document-node())/…`,
                    // so the initial context node is read to find its root —
                    // "The effect of this initial step is to begin the path at
                    // the root node of the tree that contains the context
                    // node." Every explicit step then runs in an inner focus.
                    self.note_focus(FocusUse::ITEM, in_initial_focus);
                    for &step_id in &path.steps {
                        self.walk(step_id, false);
                    }
                } else {
                    // Only the first step of a relative path sees the initial
                    // focus; each following step is `E2` of some `E1/E2`.
                    for (index, &step_id) in path.steps.iter().enumerate() {
                        self.walk(step_id, in_initial_focus && index == 0);
                    }
                }
            }

            // §3.3.2: the primary expression is evaluated in the enclosing
            // focus, the predicates in an inner one.
            AstNode::FilterExpr(filter) => {
                self.walk(filter.base, in_initial_focus);
                for &pred_id in &filter.predicates {
                    self.walk(pred_id, false);
                }
            }

            AstNode::Range(range) => {
                self.walk(range.start, in_initial_focus);
                self.walk(range.end, in_initial_focus);
            }

            AstNode::UnaryOp(unary) => self.walk(unary.operand, in_initial_focus),

            AstNode::BinaryOp(binary) => {
                self.walk(binary.left, in_initial_focus);
                self.walk(binary.right, in_initial_focus);
            }

            // An axis step evaluates its axis against the context node; it is
            // only reached with `in_initial_focus` when it is the first step of
            // a relative path. §3.2.2: its predicates get an inner focus.
            AstNode::PathStep(step) => {
                self.note_focus(FocusUse::ITEM, in_initial_focus);
                for &pred_id in &step.predicates {
                    self.walk(pred_id, false);
                }
            }

            // `instance of` / `treat as` / `cast as` / `castable as`, and the
            // constructor functions the binder rewrites into `cast as`.
            AstNode::TypeExpr(type_expr) => self.walk(type_expr.operand, in_initial_focus),
        }
    }

    /// Record a focus read, but only when it happens in the initial focus.
    #[inline]
    fn note_focus(&mut self, use_: FocusUse, in_initial_focus: bool) {
        if !in_initial_focus {
            return;
        }
        self.deps.uses_focus |= use_.item;
        self.deps.uses_position |= use_.position;
        self.deps.uses_last |= use_.size;
    }

    /// Record a reference to an external variable slot; slots at or above the
    /// external boundary belong to the expression itself and are collected
    /// separately, in `inner_var_refs`.
    #[inline]
    fn mark_var(&mut self, slot: VarSlotId) {
        if (slot as usize) >= self.external_slot_count {
            // A handful of range variables at most, so a linear scan beats a
            // set and keeps the order stable (source order).
            if !self.deps.inner_var_refs.contains(&slot) {
                self.deps.inner_var_refs.push(slot);
            }
            return;
        }
        let word = (slot / 64) as usize;
        if let Some(bits) = self.deps.referenced_vars.get_mut(word) {
            *bits |= 1u64 << (slot % 64);
        }
    }

    /// Add a distinct `(namespace, local name, arity)` entry for a call site.
    fn record_call(&mut self, call: &FunctionCallNode) {
        let entry = FunctionCallRef {
            namespace: self.bound_namespace(call),
            local_name: call.local_name.clone(),
            arity: call.args.len(),
        };
        // Expressions have a handful of distinct calls; a linear scan beats a
        // hash set here and keeps the reported order stable (source order).
        if !self.deps.function_calls.contains(&entry) {
            self.deps.function_calls.push(entry);
        }
    }

    /// The namespace the call actually **bound** to.
    ///
    /// Taken from the signature of the resolved function, so the reported
    /// namespace is the one the function is declared in rather than the way
    /// the call happened to be spelled. That matters in XPath 1.0 mode, where
    /// `XPath10Catalog` resolves an unprefixed core function through
    /// `FN_NAMESPACE` while the static context's default function namespace is
    /// the empty string (`functions/extensible.rs:577-590`,
    /// `context.rs:167-175`), and for the `FN_2010_NAMESPACE` alias, which the
    /// registry maps onto the same entries (`functions/registry.rs:110-140`).
    ///
    /// Falls back to the lexical resolution — the same one `bind_node` uses
    /// (`bind.rs:146-154`) — only when no signature is reachable, i.e. when the
    /// node carries no handle (an unbound tree) or the catalog does not know
    /// the handle.
    fn bound_namespace(&self, call: &FunctionCallNode) -> String {
        if let Some(handle) = call.function_handle {
            // Ask the catalog that bound the call; it knows custom functions too.
            if let Some(signature) = self.ctx.function_catalog().get_signature(handle) {
                return signature.namespace.to_string();
            }
            // Defensive: a catalog that cannot describe its own built-in
            // handles still resolves through the built-in registry.
            if let Ok(id) = handle_to_function_id(handle) {
                if let Some(entry) = FUNCTION_REGISTRY.by_id(id) {
                    return entry.signature.namespace.to_string();
                }
            }
        }
        if call.prefix.is_empty() {
            self.ctx.default_function_namespace().to_string()
        } else {
            self.ctx.resolve_prefix(&call.prefix).unwrap_or_default()
        }
    }
}

/// Which focus components a *call site* of a built-in function reads.
///
/// Derived from the implementations, not from the specification's prose, so
/// that the flags describe what this engine actually does. Every entry below
/// was read off the source:
///
/// | Call | Reads | Source |
/// |---|---|---|
/// | `fn:position()` | context position | `functions/special.rs:38-43` (`context.context_position`) |
/// | `fn:last()` | context size | `functions/special.rs:60-65` (`context.context_size`) |
/// | `fn:string()` | context item | `functions/mod.rs:931-944` (`require_context_item`), XPath 1.0: `functions/extensible.rs:637-645` |
/// | `fn:number()` | context item | `functions/mod.rs:958-974` (`require_context_item`), XPath 1.0: `functions/extensible.rs:661-672` |
/// | `fn:string-length()` | context item | `functions/string.rs:122-146` (`context.context_item`) |
/// | `fn:normalize-space()` | context item | `functions/string.rs:159-186` (`context.context_item`) |
/// | `fn:name()` | context node | `functions/node.rs:35-43` → `get_node_arg`, `functions/node.rs:550-563` |
/// | `fn:local-name()` | context node | `functions/node.rs:59-71` → `get_node_arg` |
/// | `fn:namespace-uri()` | context node | `functions/node.rs:87-99` → `get_node_arg` |
/// | `fn:base-uri()` | context node | `functions/node.rs:229-241` → `get_node_arg` |
/// | `fn:root()` | context node | `functions/node.rs:362-370` → `get_node_arg` |
/// | `fn:lang($testlang)` | context node | `functions/node.rs:301-325` (1-arity form falls back to `context.context_item`) |
/// | `fn:id($arg)` | context node | `functions/node.rs` `id` → `target_document` (1-arity form takes the reference node from `context.context_item`) |
/// | `fn:idref($arg)` | context node | `functions/node.rs` `idref` → `target_document` (likewise) |
///
/// Functions whose *only* registered form takes the node explicitly are
/// deliberately absent even though their implementations route through
/// `get_node_arg`: `fn:node-name`, `fn:nilled` and `fn:document-uri` are
/// registered with `FunctionSignature::new` (exact arity 1) in
/// `functions/registry.rs:617,623,645`, so a 0-arity call does not bind.
///
/// A call this function cannot classify — an unbound node, or a custom
/// function registered through a [`FunctionCatalog`](crate::xpath::functions::FunctionCatalog)
/// whose body this crate cannot inspect — is conservatively reported as
/// reading the context item, so that a caller skipping focus setup on
/// `uses_focus() == false` can never be wrong. `uses_position()` and
/// `uses_last()` stay narrow by design: they mean "`fn:position()` /
/// `fn:last()` is called in the initial focus".
fn call_focus_use(call: &FunctionCallNode) -> FocusUse {
    let Some(handle) = call.function_handle else {
        // Not bound (or rewritten): assume the worst.
        return FocusUse::ITEM;
    };
    if !handle.is_builtin() {
        // Custom function: its body is opaque to this analysis.
        return FocusUse::ITEM;
    }
    let Ok(id) = handle_to_function_id(handle) else {
        return FocusUse::ITEM;
    };

    let arity = call.args.len();
    match (id, arity) {
        (FunctionId::Position, 0) => FocusUse::POSITION,
        (FunctionId::Last, 0) => FocusUse::SIZE,
        (
            FunctionId::String
            | FunctionId::Number
            | FunctionId::StringLength
            | FunctionId::NormalizeSpace
            | FunctionId::Name
            | FunctionId::LocalName
            | FunctionId::NamespaceUri
            | FunctionId::BaseUri
            | FunctionId::Root,
            0,
        ) => FocusUse::ITEM,
        (FunctionId::Lang | FunctionId::Id | FunctionId::Idref, 1) => FocusUse::ITEM,
        _ => FocusUse::NONE,
    }
}

// ============================================================================
// Run invariance
// ============================================================================

/// A set of variable slots, held as a bitset.
#[derive(Debug, Clone, Default)]
pub(crate) struct SlotSet {
    words: Vec<u64>,
}

impl SlotSet {
    /// Whether `slot` is in the set. O(1).
    #[inline]
    pub(crate) fn contains(&self, slot: VarSlotId) -> bool {
        let word = (slot / 64) as usize;
        match self.words.get(word) {
            Some(bits) => bits & (1u64 << (slot % 64)) != 0,
            None => false,
        }
    }

    fn insert(&mut self, slot: VarSlotId) {
        let word = (slot / 64) as usize;
        if self.words.len() <= word {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1u64 << (slot % 64);
    }
}

/// Every variable slot bound by a `for`, `some` or `every` **anywhere** in
/// `arena`.
///
/// A slot outside this set is bound before evaluation starts — by
/// `XPathEvaluator`'s pending variables or by a host's setup callback — and the
/// engine never writes it again while the expression runs, so a reference to it
/// yields the same value however often it is evaluated. A slot inside it is a
/// range variable, whose value changes with the iteration it belongs to.
///
/// The set is deliberately a property of the whole arena rather than of one
/// scope: a reference to a range variable is then rejected wherever it appears,
/// which is conservative — a `for` wholly inside the analysed subtree is
/// rejected as well — and cannot mistake an outer binding for an inner one.
pub(crate) fn range_var_slots(arena: &AstArena) -> SlotSet {
    let mut set = SlotSet::default();
    for (_, node) in arena.iter() {
        let bindings = match node {
            AstNode::For(for_node) => &for_node.bindings,
            AstNode::Quantified(quantified) => &quantified.bindings,
            _ => continue,
        };
        for binding in bindings {
            if let Some(slot) = binding.slot {
                set.insert(slot);
            }
        }
    }
    set
}

/// The variable slots the subtree rooted at `root` reads, if that subtree
/// produces the same value every time it is evaluated during one run of the
/// enclosing expression — and `None` if it does not.
///
/// `Some` when the subtree
///
/// * reads no part of the focus it is entered with, so re-evaluating it in a
///   predicate, a later path step, a `for` body or a quantified body cannot
///   change its value — this is exactly [`ExprDependencies::uses_focus`] applied
///   to the subtree, with the focus rules documented at the top of this module;
/// * calls no function: a function result may depend on host state that this
///   crate cannot inspect (a custom
///   [`FunctionEvaluator`](crate::xpath::functions::FunctionEvaluator)), or on
///   dynamic-context state this analysis does not model, and a call may have
///   effects that skipping the evaluation would skip too;
/// * references no range variable, i.e. no slot in `range_vars`.
///
/// What is left — variable references bound before the run, literals, ranges,
/// sequence constructors, arithmetic, `if`, type expressions, and paths and
/// predicates rooted in any of those — is a pure function of state that does not
/// change while the expression runs. Note that the walk is conservative in the
/// other direction too: an unclassifiable construct reports a focus read rather
/// than staying silent.
///
/// The returned slots are the ones whose bindings the value depends on, which is
/// what a caller that caches the value needs in order to notice a rebinding.
pub(crate) fn run_invariant_vars(
    arena: &AstArena,
    root: AstNodeId,
    ctx: &XPathContext<'_>,
    range_vars: &SlotSet,
) -> Option<Vec<VarSlotId>> {
    // A boundary of 0 puts *every* variable reference of the subtree into
    // `inner_var_refs`, which is what the range-variable test below needs.
    let deps = analyze(arena, root, ctx, 0);
    if deps.uses_focus() || !deps.function_calls().is_empty() {
        return None;
    }
    if deps
        .inner_var_refs
        .iter()
        .any(|&slot| range_vars.contains(slot))
    {
        return None;
    }
    Some(deps.inner_var_refs)
}

/// Whether the subtree rooted at `root` is run invariant; see
/// [`run_invariant_vars`], of which this is the predicate form.
#[cfg(test)]
pub(crate) fn is_run_invariant(
    arena: &AstArena,
    root: AstNodeId,
    ctx: &XPathContext<'_>,
    range_vars: &SlotSet,
) -> bool {
    run_invariant_vars(arena, root, ctx, range_vars).is_some()
}

#[cfg(test)]
#[path = "deps_tests.rs"]
mod deps_tests;
