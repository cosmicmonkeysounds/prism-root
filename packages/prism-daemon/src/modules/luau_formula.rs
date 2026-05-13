//! Luau-backed formula resolver for computed fields.
//!
//! Phase 4.4 of `docs/dev/luau-integration-plan.md`. The pure-Rust
//! formula evaluator in `prism_core::language::expression` covers the
//! standard arithmetic + builtin grammar; this module is the escape
//! hatch for formulas that need to read related objects, call helper
//! libraries, or compose with the rest of the Luau surface.
//!
//! ```text
//! formula = "luau: self.tasks[1].title"
//! ```
//!
//! `LuauFormulaEvaluator::evaluate(object)` binds the object's JSON
//! representation as `self`, runs the body through
//! [`luau_module::eval`], and returns the resulting JSON. Same per-call
//! `Lua` lifecycle as `LuauActionHandler` — no shared state across
//! invocations, persistent state belongs on the object graph.
//!
//! Hosts plug this into their `resolve_formula_field` path by checking
//! the formula prefix and dispatching to the matching evaluator:
//!
//! ```rust,ignore
//! if let Some(body) = formula.strip_prefix("luau:") {
//!     return LuauFormulaEvaluator::new(body.trim()).evaluate(&obj);
//! }
//! // … fall through to the pure-Rust evaluator …
//! ```

use prism_core::foundation::object_model::types::GraphObject;
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::modules::luau_module;
use crate::modules::prism_context::PrismContext;

/// A Luau-backed formula evaluator. Stores the body verbatim so the
/// host can swap it on hot-reload without re-allocating supporting
/// state.
#[derive(Debug, Clone)]
pub struct LuauFormulaEvaluator {
    body: String,
}

impl LuauFormulaEvaluator {
    /// Build an evaluator that runs `body` as a Luau expression. The
    /// body should be a single expression suitable for the `return
    /// (...)` wrapper that [`luau_module::eval`] applies.
    pub fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }

    /// Evaluate the formula against `object`. `self` is bound as the
    /// serialised object, so authors can write
    /// `self.data.title`-style accessors directly.
    pub fn evaluate(&self, object: &GraphObject) -> Result<JsonValue, String> {
        self.evaluate_with_context(object, PrismContext::default)
    }

    /// Evaluate with a host-supplied [`PrismContext`] factory.
    /// Mirror of [`crate::LuauActionHandler::with_context_fn`].
    pub fn evaluate_with_context<F>(
        &self,
        object: &GraphObject,
        ctx_fn: F,
    ) -> Result<JsonValue, String>
    where
        F: FnOnce() -> PrismContext,
    {
        let mut args = JsonMap::new();
        let self_value =
            serde_json::to_value(object).map_err(|e| format!("serialise object: {e}"))?;
        args.insert("self".into(), self_value);
        let wrapped = format!("return ({})", self.body);
        luau_module::exec_with_context(&wrapped, Some(&args), ctx_fn())
    }

    /// Convenience: dispatch on the `luau:` prefix. Returns
    /// `Some(Ok(value))` when the prefix matches and evaluation
    /// succeeded, `Some(Err(_))` on Luau error, and `None` when the
    /// formula isn't a Luau formula (caller should fall back to the
    /// pure-Rust evaluator).
    pub fn maybe_evaluate(
        formula: &str,
        object: &GraphObject,
    ) -> Option<Result<JsonValue, String>> {
        let body = formula.strip_prefix("luau:")?.trim();
        Some(Self::new(body).evaluate(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::foundation::object_model::types::GraphObject;
    use serde_json::json;

    fn sample_task() -> GraphObject {
        // GraphObject::new sets the required `id` / `type` / `name`
        // fields; the rest carry sensible defaults the test then
        // mutates.
        let mut obj = GraphObject::new("task-1", "task", "Ship Luau formulas");
        obj.data.insert("priority".into(), json!(3));
        obj.data.insert("done".into(), json!(false));
        obj
    }

    #[test]
    fn evaluate_returns_field_value() {
        let ev = LuauFormulaEvaluator::new("self.name");
        let v = ev.evaluate(&sample_task()).expect("ok");
        assert_eq!(v, json!("Ship Luau formulas"));
    }

    #[test]
    fn evaluate_can_compute_over_data() {
        let ev = LuauFormulaEvaluator::new("self.data.priority * 10");
        let v = ev.evaluate(&sample_task()).expect("ok");
        // `lua_to_json` always emits integers when the value rounds
        // exactly, so the result lands as `Number(30)` — assert on
        // `as_f64()` rather than the literal to stay tolerant.
        assert_eq!(v.as_f64(), Some(30.0));
    }

    #[test]
    fn maybe_evaluate_dispatches_only_on_luau_prefix() {
        let prefixed = LuauFormulaEvaluator::maybe_evaluate("luau: self.name", &sample_task())
            .expect("Some")
            .expect("ok");
        assert_eq!(prefixed, json!("Ship Luau formulas"));
        // Bare expressions don't match — caller falls through to the
        // pure-Rust grammar.
        assert!(LuauFormulaEvaluator::maybe_evaluate("price * qty", &sample_task()).is_none());
    }

    #[test]
    fn evaluate_propagates_lua_error() {
        let ev = LuauFormulaEvaluator::new("error('busted')");
        let err = ev.evaluate(&sample_task()).expect_err("expected error");
        assert!(err.contains("busted"), "unexpected error: {err}");
    }

    #[test]
    fn evaluate_sees_prism_global() {
        let ev = LuauFormulaEvaluator::new("prism.tokens.colors.accent.r");
        let v = ev.evaluate(&sample_task()).expect("ok");
        assert_eq!(v.as_f64(), Some(110.0));
    }
}
