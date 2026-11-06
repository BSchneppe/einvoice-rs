//! Warnings the XPath expressions of XSD 1.1 assertions and type alternatives
//! raise while an instance is validated.
//!
//! An expression's warning (an `xpath::XPathWarning`, e.g. `xs-seconds-capacity` for
//! a date/time or duration result that keeps its seconds truncated) belongs to
//! the validation run that evaluated it: the run reports it to its sink as it
//! reports the same code for a literal. The expressions are evaluated deep
//! inside value validation — a simple type's `xs:assertion` facets run from
//! `simple::validate_simple_type_in` — so the runtime opens a collection
//! around the call (`collecting`) and the evaluating code hands each warning
//! to it (`note`). Outside a collection —
//! an XPath cast that checks a user type's facets, a schema-time check — a
//! warning is dropped.

use std::cell::RefCell;

/// One warning an expression raised during validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpressionWarning {
    pub(crate) code: &'static str,
    /// The expression that raised it, e.g. `assertion '$value gt 0'`.
    pub(crate) source: String,
    pub(crate) message: String,
}

thread_local! {
    /// The innermost collection in progress, if any.
    static PENDING: RefCell<Option<Vec<ExpressionWarning>>> = const { RefCell::new(None) };
}

/// Run `evaluate`, and return what it returned with the warnings the
/// expressions it evaluated raised, in order. A collection inside another
/// collects its own; the outer one resumes afterwards (also on unwind).
pub(crate) fn collecting<R>(evaluate: impl FnOnce() -> R) -> (R, Vec<ExpressionWarning>) {
    struct Restore(Option<Option<Vec<ExpressionWarning>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                PENDING.with(|pending| *pending.borrow_mut() = previous);
            }
        }
    }
    let previous = PENDING.with(|pending| pending.replace(Some(Vec::new())));
    let restore = Restore(Some(previous));
    let result = evaluate();
    let collected = PENDING
        .with(|pending| pending.borrow_mut().take())
        .unwrap_or_default();
    drop(restore);
    (result, collected)
}

/// Hand a warning to the innermost collection in progress; dropped without
/// one.
#[cfg_attr(not(feature = "xsd11"), allow(dead_code))]
pub(crate) fn note(code: &'static str, source: impl FnOnce() -> String, message: String) {
    PENDING.with(|pending| {
        if let Some(warnings) = pending.borrow_mut().as_mut() {
            warnings.push(ExpressionWarning {
                code,
                source: source(),
                message,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_nest_and_drop_outside() {
        note("x-outside", String::new, "dropped".to_string());
        let ((), outer) = collecting(|| {
            note("x-outer", String::new, "one".to_string());
            let ((), inner) = collecting(|| note("x-inner", String::new, "two".to_string()));
            assert_eq!(inner.len(), 1);
            assert_eq!(inner[0].code, "x-inner");
            note("x-outer", String::new, "three".to_string());
        });
        let codes: Vec<_> = outer.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(codes, ["one", "three"]);
        let ((), none) = collecting(|| ());
        assert!(none.is_empty());
    }

    #[test]
    fn a_panic_restores_the_outer_collection() {
        let ((), outer) = collecting(|| {
            let caught = std::panic::catch_unwind(|| {
                collecting(|| {
                    note("x-inner", String::new, "lost".to_string());
                    panic!("inside");
                })
            });
            assert!(caught.is_err());
            note("x-outer", String::new, "kept".to_string());
        });
        assert_eq!(outer.len(), 1);
        assert_eq!(outer[0].message, "kept");
    }
}
