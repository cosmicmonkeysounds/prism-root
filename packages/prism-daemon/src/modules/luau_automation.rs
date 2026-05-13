//! Luau-backed [`ActionHandler`] for [`AutomationEngine`].
//!
//! Phase 4.3 of `docs/dev/luau-integration-plan.md`. Connects user
//! `.luau` files declared in `.prism.json`'s `scripts.automations`
//! glob to `prism_core::kernel::automation::AutomationEngine` —
//! `prism_builder::load_automations` walks the glob at boot and the
//! host wraps each compiled body in a [`LuauActionHandler`] before
//! registering it with the engine.
//!
//! The handler is `Send + Sync`: it stores a Luau source string and
//! builds a fresh `Lua` state per `handle` call through
//! [`luau_module::exec_with_context`]. That matches `mlua::Lua`'s
//! `!Send` constraint without forcing the caller to do its own
//! synchronisation.
//!
//! Two ergonomics choices, both intentional:
//!
//! 1. **Stateless per-call.** Module-level `local`s inside the script
//!    don't survive across invocations. Persistent state belongs on
//!    `prism.store` / the object graph — the same constraint the
//!    widget hot-reload path makes (see plan §"Hot-reload — per-component
//!    re-registration").
//!
//! 2. **Default context.** The handler ships a [`PrismContext::default()`]
//!    today. Hosts that want to attach a live `CollectionStore` (so a
//!    Luau automation can mutate the object graph) call
//!    [`LuauActionHandler::with_context_fn`] and return a fresh
//!    `PrismContext` per invocation. The plan calls for a richer
//!    `automation`-scoped context (a stand-in `prism.commands`,
//!    cross-actor messaging) — those slot in here as the host wires
//!    them up.

use std::sync::Arc;

use prism_core::kernel::automation::{ActionHandler, AutomationAction, AutomationContext};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::modules::luau_module;
use crate::modules::prism_context::PrismContext;

/// A Luau script that handles automation actions through the
/// [`ActionHandler`] trait. Stores its source verbatim so hot-reload
/// can swap the inner string without touching the engine
/// registration.
pub struct LuauActionHandler {
    source: String,
    context_fn: Arc<dyn Fn() -> PrismContext + Send + Sync>,
}

impl std::fmt::Debug for LuauActionHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuauActionHandler")
            .field("source_len", &self.source.len())
            .finish()
    }
}

impl LuauActionHandler {
    /// Build a handler that runs `source` with a default
    /// [`PrismContext`] (tokens / shell mode / permission / config).
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            context_fn: Arc::new(PrismContext::default),
        }
    }

    /// Replace the per-call `PrismContext` factory. Useful when the
    /// host wants automations to see the live collection through
    /// `PrismContext::with_collection(...)`.
    pub fn with_context_fn(mut self, f: Arc<dyn Fn() -> PrismContext + Send + Sync>) -> Self {
        self.context_fn = f;
        self
    }

    /// The Luau source the handler executes. Surfaced for hot-reload
    /// callers that want to inspect the current body without
    /// re-reading the underlying file.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl ActionHandler for LuauActionHandler {
    fn handle(&self, action: &AutomationAction, context: &AutomationContext) -> Result<(), String> {
        let mut args = JsonMap::new();
        let action_value =
            serde_json::to_value(action).map_err(|e| format!("serialise action: {e}"))?;
        let context_value =
            serde_json::to_value(context).map_err(|e| format!("serialise context: {e}"))?;
        args.insert("action".into(), action_value);
        args.insert("context".into(), context_value);
        let ctx = (self.context_fn)();
        let _: JsonValue = luau_module::exec_with_context(&self.source, Some(&args), ctx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::kernel::automation::AutomationAction;
    use serde_json::json;

    fn ctx() -> AutomationContext {
        AutomationContext {
            automation_id: "auto-1".into(),
            triggered_at: "2026-05-13T00:00:00Z".into(),
            trigger_type: "object:update".into(),
            object: Some({
                let mut m = JsonMap::new();
                m.insert("id".into(), json!("task-1"));
                m.insert("status".into(), json!("done"));
                m
            }),
            previous_object: None,
            extra: None,
        }
    }

    fn delay_action() -> AutomationAction {
        AutomationAction::Delay { seconds: 0.0 }
    }

    #[test]
    fn handler_runs_and_returns_ok() {
        let handler = LuauActionHandler::new(
            r#"
            -- Reading action + context proves both are bound as globals.
            assert(action.type == 'delay')
            assert(context.automationId == 'auto-1')
            return nil
            "#,
        );
        handler.handle(&delay_action(), &ctx()).expect("handle ok");
    }

    #[test]
    fn handler_surfaces_lua_error_as_string() {
        let handler = LuauActionHandler::new("error('nope')");
        let err = handler
            .handle(&delay_action(), &ctx())
            .expect_err("expected error");
        assert!(err.contains("nope"), "unexpected error: {err}");
    }

    #[test]
    fn handler_sees_object_payload() {
        let handler = LuauActionHandler::new(
            r#"
            assert(context.object.id == 'task-1')
            assert(context.object.status == 'done')
            return nil
            "#,
        );
        handler.handle(&delay_action(), &ctx()).expect("handle ok");
    }

    #[test]
    fn handler_sees_prism_global() {
        let handler = LuauActionHandler::new(
            r#"
            -- Default context still exposes tokens, so an automation can
            -- read the accent colour for a notification action.
            assert(prism.tokens.colors.accent.r ~= nil)
            return nil
            "#,
        );
        handler.handle(&delay_action(), &ctx()).expect("handle ok");
    }
}
