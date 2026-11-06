//! Streaming matcher for identity-constraint XPath expressions.
//!
//! `ActiveAxis` advances through a compiled `Asttree` as SAX-style element
//! events arrive, reporting matches without buffering the document.

#![allow(dead_code)]

use crate::ids::NameId;

use super::asttree::{AstStep, Asttree};

/// Per-depth matching state for a single path.
#[derive(Clone)]
struct MatchContext {
    /// Step indices waiting to match a child element at the next depth.
    active_steps: Vec<usize>,
    /// Step indices that completed (element match) or are pending attribute check
    /// at this depth. For complete matches the index points at the last `Child`
    /// step; for attribute-pending matches it points at the `Attribute` step.
    matched_here: Vec<usize>,
    /// Whether an attribute tail step is pending (e.g. `foo/@id`).
    awaiting_attribute: bool,
    /// Whether to re-inject `first_real_step` at every depth (`.//` descendant).
    try_from_start: bool,
}

/// State for a single path alternative within the `Asttree` union.
struct PathState {
    /// One context per element depth.
    context_stack: Vec<MatchContext>,
    /// First non-`SelfNode` step index.
    first_real_step: usize,
    /// Cached from [`AstPath::descendant`](super::asttree::AstPath::descendant).
    has_descendant_prefix: bool,
}

/// Streaming matcher that advances through a compiled identity-constraint XPath
/// as element open/close events arrive.
pub(crate) struct ActiveAxis {
    ast: Asttree,
    current_depth: i32,
    active: bool,
    path_states: Vec<PathState>,
    last_entered_match: bool,
    last_exited_match: bool,
    scope_match_flag: bool,
}

/// Advance past consecutive `SelfNode` steps, returning the index of the
/// first non-`SelfNode` step (or `steps.len()` if all remaining are `SelfNode`).
fn skip_self_nodes(steps: &[AstStep], mut idx: usize) -> usize {
    while idx < steps.len() && matches!(steps[idx], AstStep::SelfNode) {
        idx += 1;
    }
    idx
}

impl ActiveAxis {
    /// Create a new matcher from a compiled `Asttree`.
    pub(crate) fn new(ast: Asttree) -> Self {
        let path_states = ast
            .paths
            .iter()
            .map(|_| PathState {
                context_stack: Vec::new(),
                first_real_step: 0,
                has_descendant_prefix: false,
            })
            .collect();

        Self {
            ast,
            current_depth: -1,
            active: false,
            path_states,
            last_entered_match: false,
            last_exited_match: false,
            scope_match_flag: false,
        }
    }

    /// Initialize matching state for the scope element.
    ///
    /// Returns `true` if the expression is a bare `.` (immediate scope match).
    pub(crate) fn activate(&mut self) -> bool {
        self.active = true;
        self.current_depth = 0;
        self.scope_match_flag = false;
        self.last_entered_match = false;
        self.last_exited_match = false;

        for (i, path) in self.ast.paths.iter().enumerate() {
            let first_real = path
                .steps
                .iter()
                .position(|s| !matches!(s, AstStep::SelfNode))
                .unwrap_or(path.steps.len());

            let state = &mut self.path_states[i];
            state.first_real_step = first_real;
            state.has_descendant_prefix = path.descendant;
            state.context_stack.clear();

            if first_real >= path.steps.len() {
                // Bare "." — matches the scope element itself.
                self.scope_match_flag = true;
                state.context_stack.push(MatchContext {
                    active_steps: vec![],
                    matched_here: vec![],
                    awaiting_attribute: false,
                    try_from_start: false,
                });
            } else if matches!(path.steps[first_real], AstStep::Attribute(_)) && !path.descendant {
                // `./@attr` — SelfNode followed by an attribute step (non-descendant).
                // The attribute belongs to the scope element, so mark it
                // awaiting immediately (no `move_to_start_element` needed).
                state.context_stack.push(MatchContext {
                    active_steps: vec![],
                    matched_here: vec![first_real],
                    awaiting_attribute: true,
                    try_from_start: false,
                });
            } else {
                state.context_stack.push(MatchContext {
                    active_steps: vec![first_real],
                    matched_here: vec![],
                    awaiting_attribute: false,
                    try_from_start: path.descendant,
                });
            }
        }

        self.scope_match_flag
    }

    /// Advance depth tracking for an element opening inside a
    /// processContents="skip" wildcard subtree, without attempting any
    /// selector / field matches. Pushes an empty match context so descendant
    /// axes can't expand into the skipped content. Used by IC processing in
    /// XSD 1.1 to honour wildcard attribution: skipped elements are outside
    /// the schema's validation scope and must not contribute to keys/uniques
    /// or keyrefs (see wild101..103).
    pub(crate) fn move_to_skipped_element(&mut self) {
        if !self.active {
            return;
        }
        self.current_depth += 1;
        self.last_entered_match = false;
        for state in &mut self.path_states {
            state.context_stack.push(MatchContext {
                active_steps: Vec::new(),
                matched_here: Vec::new(),
                awaiting_attribute: false,
                try_from_start: false,
            });
        }
    }

    /// Advance matching on an element open event.
    ///
    /// Returns `true` if any path completed (or attribute-pending) a match at
    /// this element.
    pub(crate) fn move_to_start_element(&mut self, local_name: NameId, ns: NameId) -> bool {
        if !self.active {
            return false;
        }

        self.current_depth += 1;
        self.last_entered_match = false;

        for (path_idx, path) in self.ast.paths.iter().enumerate() {
            let state = &mut self.path_states[path_idx];

            // Get candidate steps from parent context.
            let parent_ctx = match state.context_stack.last() {
                Some(ctx) => ctx,
                None => {
                    // Should not happen while active, but be defensive.
                    state.context_stack.push(MatchContext {
                        active_steps: vec![],
                        matched_here: vec![],
                        awaiting_attribute: false,
                        try_from_start: state.has_descendant_prefix,
                    });
                    continue;
                }
            };

            let mut candidates: Vec<usize> = parent_ctx.active_steps.clone();

            // For descendant paths, also try from the first real step (dedup).
            if parent_ctx.try_from_start && !candidates.contains(&state.first_real_step) {
                candidates.push(state.first_real_step);
            }

            let mut new_active = Vec::new();
            let mut new_matched = Vec::new();
            let mut new_awaiting = false;

            for &s in &candidates {
                if s >= path.steps.len() {
                    continue;
                }
                match &path.steps[s] {
                    AstStep::SelfNode => {
                        // SelfNode transparently matches the current node.
                        // Advance to the next real step.
                        let next = skip_self_nodes(&path.steps, s + 1);
                        if next >= path.steps.len() {
                            new_matched.push(s);
                        } else {
                            match &path.steps[next] {
                                AstStep::Attribute(_) => {
                                    new_awaiting = true;
                                    new_matched.push(next);
                                }
                                _ => {
                                    if !new_active.contains(&next) {
                                        new_active.push(next);
                                    }
                                }
                            }
                        }
                    }
                    AstStep::Child(name_test) => {
                        if name_test.matches(ns, local_name) {
                            // Skip any trailing SelfNode steps after this match.
                            let next = skip_self_nodes(&path.steps, s + 1);
                            if next >= path.steps.len() {
                                // Complete element match — no more steps.
                                new_matched.push(s);
                            } else {
                                match &path.steps[next] {
                                    AstStep::Attribute(_) => {
                                        new_awaiting = true;
                                        new_matched.push(next);
                                    }
                                    _ => {
                                        new_active.push(next);
                                    }
                                }
                            }
                        }
                    }
                    AstStep::Attribute(_) => {
                        // Attribute steps are not matched against elements.
                    }
                }
            }

            if !new_matched.is_empty() {
                self.last_entered_match = true;
            }

            state.context_stack.push(MatchContext {
                active_steps: new_active,
                matched_here: new_matched,
                awaiting_attribute: new_awaiting,
                try_from_start: state.has_descendant_prefix,
            });
        }

        self.last_entered_match
    }

    /// Pop matching context on an element close event.
    ///
    /// Returns `true` if the element being closed had any matches.
    /// Deactivates when the scope element closes (depth goes below 0).
    pub(crate) fn end_element(&mut self) -> bool {
        if !self.active {
            return false;
        }

        self.last_exited_match = false;

        for state in &mut self.path_states {
            if let Some(ctx) = state.context_stack.pop() {
                // Only count element-complete matches.  Contexts with
                // `awaiting_attribute` hold unconsumed attribute steps —
                // those must not trigger `exited_match` because the
                // attribute was never actually present on the element.
                if !ctx.matched_here.is_empty() && !ctx.awaiting_attribute {
                    self.last_exited_match = true;
                }
            }
        }

        self.current_depth -= 1;
        if self.current_depth < 0 {
            // Exiting the scope element itself.
            if self.scope_match_flag {
                self.last_exited_match = true;
            }
            self.active = false;
        }

        self.last_exited_match
    }

    /// Check whether the given attribute matches a pending attribute step.
    ///
    /// Call this after `move_to_start_element` returned `true` when the
    /// compiled expression ends with an attribute axis (e.g. `foo/@id`).
    pub(crate) fn matches_attribute(&self, local_name: NameId, ns: NameId) -> bool {
        if !self.active {
            return false;
        }

        for (path_idx, path) in self.ast.paths.iter().enumerate() {
            let state = &self.path_states[path_idx];
            if let Some(ctx) = state.context_stack.last() {
                if ctx.awaiting_attribute {
                    for &step_idx in &ctx.matched_here {
                        if step_idx < path.steps.len() {
                            if let AstStep::Attribute(name_test) = &path.steps[step_idx] {
                                if name_test.matches(ns, local_name) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Reset for reuse with a new scope element.
    pub(crate) fn reactivate(&mut self) {
        self.active = false;
        self.current_depth = -1;
        self.scope_match_flag = false;
        self.last_entered_match = false;
        self.last_exited_match = false;
        for state in &mut self.path_states {
            state.context_stack.clear();
        }
    }

    /// Whether the matcher is within the constraint scope.
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the last `move_to_start_element` produced a match.
    pub(crate) fn entered_match(&self) -> bool {
        self.last_entered_match
    }

    /// Whether the last `end_element` left a matched scope.
    pub(crate) fn exited_match(&self) -> bool {
        self.last_exited_match
    }

    /// Whether the expression is a bare `.` that matches the scope element.
    pub(crate) fn scope_match(&self) -> bool {
        self.scope_match_flag
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::context::NamespaceContextSnapshot;
    use crate::namespace::table::{well_known, NameTable};
    use crate::schema::model::XsdVersion;
    use crate::validation::asttree::Asttree;

    fn compile_sel(xpath: &str, table: &NameTable) -> Asttree {
        let snap = NamespaceContextSnapshot::default();
        Asttree::compile_selector(xpath, &snap, table, None, None, None, XsdVersion::V1_0).unwrap()
    }

    fn compile_fld(xpath: &str, table: &NameTable) -> Asttree {
        let snap = NamespaceContextSnapshot::default();
        Asttree::compile_field(xpath, &snap, table, None, None, None, XsdVersion::V1_0).unwrap()
    }

    fn compile_sel_with_ns(
        xpath: &str,
        table: &NameTable,
        snap: &NamespaceContextSnapshot,
    ) -> Asttree {
        Asttree::compile_selector(xpath, snap, table, None, None, None, XsdVersion::V1_0).unwrap()
    }

    /// `foo/bar`: enter `<foo>` (no match), enter `<bar>` (match), exit both.
    #[test]
    fn simple_path() {
        let table = NameTable::new();
        let ast = compile_sel("foo/bar", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");
        let bar = table.add("bar");

        assert!(!axis.activate());

        // Enter <foo> — partial match only
        assert!(!axis.move_to_start_element(foo, well_known::EMPTY));
        // Enter <bar> — complete match
        assert!(axis.move_to_start_element(bar, well_known::EMPTY));

        // Exit </bar> — exiting a matched element
        assert!(axis.end_element());
        assert!(axis.exited_match());
        // Exit </foo> — no match at this level
        assert!(!axis.end_element());
        // Exit scope
        axis.end_element();
        assert!(!axis.is_active());
    }

    /// `.//bar`: match at depth 1 and depth 3.
    #[test]
    fn descendant_any_depth() {
        let table = NameTable::new();
        let ast = compile_sel(".//bar", &table);
        let mut axis = ActiveAxis::new(ast);

        let a = table.add("a");
        let bar = table.add("bar");

        assert!(!axis.activate());

        // Match at depth 1
        assert!(axis.move_to_start_element(bar, well_known::EMPTY));
        axis.end_element();

        // Match at depth 3: <a><a><bar/>
        assert!(!axis.move_to_start_element(a, well_known::EMPTY));
        assert!(!axis.move_to_start_element(a, well_known::EMPTY));
        assert!(axis.move_to_start_element(bar, well_known::EMPTY));
        axis.end_element(); // </bar>
        axis.end_element(); // </a>
        axis.end_element(); // </a>
    }

    /// `.//a/b` with `<a><a><b/>`: nested `<a>` both produce step-1 candidates.
    #[test]
    fn overlapping_descendant() {
        let table = NameTable::new();
        let ast = compile_sel(".//a/b", &table);
        let mut axis = ActiveAxis::new(ast);

        let a = table.add("a");
        let b = table.add("b");

        assert!(!axis.activate());

        // <a> — partial match
        assert!(!axis.move_to_start_element(a, well_known::EMPTY));
        // <a> (nested) — starts a new a/b candidate via descendant
        assert!(!axis.move_to_start_element(a, well_known::EMPTY));
        // <b> — matches the inner <a><b> path
        assert!(axis.move_to_start_element(b, well_known::EMPTY));

        axis.end_element(); // </b>
        axis.end_element(); // </a>
        axis.end_element(); // </a>
    }

    /// `a|b|c`: each alternative matches independently.
    #[test]
    fn union_paths() {
        let table = NameTable::new();
        let ast = compile_sel("a|b|c", &table);
        let mut axis = ActiveAxis::new(ast);

        let a = table.add("a");
        let b = table.add("b");
        let c = table.add("c");
        let x = table.add("x");

        assert!(!axis.activate());

        assert!(axis.move_to_start_element(a, well_known::EMPTY));
        axis.end_element();

        assert!(axis.move_to_start_element(b, well_known::EMPTY));
        axis.end_element();

        assert!(axis.move_to_start_element(c, well_known::EMPTY));
        axis.end_element();

        assert!(!axis.move_to_start_element(x, well_known::EMPTY));
        axis.end_element();
    }

    /// `foo/@id`: element NOT final, `matches_attribute("id")` returns true.
    #[test]
    fn attribute_field() {
        let table = NameTable::new();
        let ast = compile_fld("foo/@id", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");
        let id = table.add("id");
        let other = table.add("other");

        assert!(!axis.activate());

        // Enter <foo> — has pending attribute
        assert!(axis.move_to_start_element(foo, well_known::EMPTY));

        // Attribute @id matches
        assert!(axis.matches_attribute(id, well_known::EMPTY));
        // Attribute @other does not
        assert!(!axis.matches_attribute(other, well_known::EMPTY));

        axis.end_element();
    }

    /// `foo/@*`: any attribute matches after `<foo>`.
    #[test]
    fn attribute_wildcard() {
        let table = NameTable::new();
        let ast = compile_fld("foo/@*", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");
        let anything = table.add("anything");
        let other = table.add("other");

        assert!(!axis.activate());

        assert!(axis.move_to_start_element(foo, well_known::EMPTY));

        // Any attribute matches @*
        assert!(axis.matches_attribute(anything, well_known::EMPTY));
        assert!(axis.matches_attribute(other, well_known::EMPTY));

        axis.end_element();
    }

    /// `.`: `activate()` returns true, `scope_match()` is true.
    #[test]
    fn self_selector() {
        let table = NameTable::new();
        let ast = compile_sel(".", &table);
        let mut axis = ActiveAxis::new(ast);

        assert!(axis.activate());
        assert!(axis.scope_match());
    }

    /// `./foo`: equivalent to `foo`, matches child.
    #[test]
    fn self_then_child() {
        let table = NameTable::new();
        let ast = compile_sel("./foo", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");
        let bar = table.add("bar");

        assert!(!axis.activate());

        // ./foo is equivalent to foo
        assert!(axis.move_to_start_element(foo, well_known::EMPTY));
        axis.end_element();

        assert!(!axis.move_to_start_element(bar, well_known::EMPTY));
        axis.end_element();
    }

    /// Prefixed name: match only with correct namespace.
    #[test]
    fn namespace_match() {
        let table = NameTable::new();
        let ns = table.add("http://example.com");
        let prefix = table.add("p");
        let snap = NamespaceContextSnapshot {
            default_ns: None,
            bindings: vec![(prefix, ns)],
        };
        let ast = compile_sel_with_ns("p:foo", &table, &snap);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");
        let other_ns = table.add("http://other.com");

        assert!(!axis.activate());

        // Correct namespace — match
        assert!(axis.move_to_start_element(foo, ns));
        axis.end_element();

        // Wrong namespace — no match
        assert!(!axis.move_to_start_element(foo, other_ns));
        axis.end_element();

        // No namespace — no match
        assert!(!axis.move_to_start_element(foo, well_known::EMPTY));
        axis.end_element();
    }

    /// `*`: any element matches.
    #[test]
    fn wildcard_match() {
        let table = NameTable::new();
        let ast = compile_sel("*", &table);
        let mut axis = ActiveAxis::new(ast);

        let any_name = table.add("anything");
        let ns = table.add("http://example.com");

        assert!(!axis.activate());

        assert!(axis.move_to_start_element(any_name, well_known::EMPTY));
        axis.end_element();

        assert!(axis.move_to_start_element(any_name, ns));
        axis.end_element();
    }

    /// After exiting scope element, `is_active()` is false.
    #[test]
    fn scope_deactivation() {
        let table = NameTable::new();
        let ast = compile_sel("foo", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");

        assert!(!axis.activate());
        assert!(axis.is_active());

        // Enter and exit a child
        assert!(axis.move_to_start_element(foo, well_known::EMPTY));
        axis.end_element();

        // Exit the scope element
        axis.end_element();
        assert!(!axis.is_active());
    }

    /// `a/b`: enter `<x><y><z>` — no matches.
    #[test]
    fn no_match_deep() {
        let table = NameTable::new();
        let ast = compile_sel("a/b", &table);
        let mut axis = ActiveAxis::new(ast);

        let x = table.add("x");
        let y = table.add("y");
        let z = table.add("z");

        assert!(!axis.activate());

        assert!(!axis.move_to_start_element(x, well_known::EMPTY));
        assert!(!axis.move_to_start_element(y, well_known::EMPTY));
        assert!(!axis.move_to_start_element(z, well_known::EMPTY));

        axis.end_element();
        axis.end_element();
        axis.end_element();
    }

    /// After deactivation, `reactivate()` + `activate()` gives fresh state.
    #[test]
    fn reactivate_reuse() {
        let table = NameTable::new();
        let ast = compile_sel("foo", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");

        // First use
        assert!(!axis.activate());
        assert!(axis.move_to_start_element(foo, well_known::EMPTY));
        axis.end_element();
        axis.end_element(); // exit scope
        assert!(!axis.is_active());

        // Reactivate for a new scope
        axis.reactivate();
        assert!(!axis.is_active());

        assert!(!axis.activate());
        assert!(axis.is_active());

        // Should match again
        assert!(axis.move_to_start_element(foo, well_known::EMPTY));
        axis.end_element();
    }

    /// `foo/./bar`: mid-path SelfNode is transparent, equivalent to `foo/bar`.
    #[test]
    fn mid_path_self_node() {
        let table = NameTable::new();
        let ast = compile_sel("foo/./bar", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");
        let bar = table.add("bar");

        assert!(!axis.activate());

        // Enter <foo> — partial match
        assert!(!axis.move_to_start_element(foo, well_known::EMPTY));
        // Enter <bar> — complete match (SelfNode skipped)
        assert!(axis.move_to_start_element(bar, well_known::EMPTY));

        axis.end_element(); // </bar>
        axis.end_element(); // </foo>
    }

    /// `foo/.`: trailing SelfNode, equivalent to `foo`.
    #[test]
    fn trailing_self_node() {
        let table = NameTable::new();
        let ast = compile_sel("foo/.", &table);
        let mut axis = ActiveAxis::new(ast);

        let foo = table.add("foo");

        assert!(!axis.activate());

        // Enter <foo> — complete match (trailing SelfNode consumed)
        assert!(axis.move_to_start_element(foo, well_known::EMPTY));

        axis.end_element();
    }

    /// `.`: exiting the scope element signals `exited_match`.
    #[test]
    fn self_selector_exit() {
        let table = NameTable::new();
        let ast = compile_sel(".", &table);
        let mut axis = ActiveAxis::new(ast);

        assert!(axis.activate());
        assert!(axis.scope_match());

        // Exit the scope element — should signal exit match
        assert!(axis.end_element());
        assert!(axis.exited_match());
        assert!(!axis.is_active());
    }

    /// `.//a`: matches `<a>` at multiple depths.
    #[test]
    fn descendant_single() {
        let table = NameTable::new();
        let ast = compile_sel(".//a", &table);
        let mut axis = ActiveAxis::new(ast);

        let a = table.add("a");
        let x = table.add("x");

        assert!(!axis.activate());

        // <a> at depth 1
        assert!(axis.move_to_start_element(a, well_known::EMPTY));
        // <a> at depth 2 (nested under first <a>)
        assert!(axis.move_to_start_element(a, well_known::EMPTY));
        axis.end_element(); // </a> inner
                            // <x> at depth 2 — no match
        assert!(!axis.move_to_start_element(x, well_known::EMPTY));
        axis.end_element(); // </x>
        axis.end_element(); // </a> outer
    }
}
