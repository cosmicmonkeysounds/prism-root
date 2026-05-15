//! `LuauService` — exec custom signal handlers + ad-hoc scripts.
//!
//! The §27 plan called for cross-service reach via
//! `services.get("luau")`; the actual implementation puts the Luau
//! runtime on `MutCtx::luau` (the shared-resource discipline from
//! §22 generalised to mutable runtimes). `SignalsService` calls
//! `ctx.luau.exec(...)` directly — same single carrier, no dynamic
//! lookup, no downcasting, type-safe at the call site.
//!
//! The trait is a shape; the implementation is feature-gated.
//! `NoopLuauHost` is the always-on fallback (returns `Null`). The
//! persistent script runtime — `prism_core::luau_runtime::LuauRuntime`
//! installed under `feature = "native"` — is a sibling of this trait:
//! it owns the long-lived `mlua::Lua` state and dispatches
//! component / service callbacks, while this `LuauHost` trait carries
//! one-shot script invocations from `&mut MutCtx` consumers
//! (`LuauService::run-selection`, custom signal handlers). A future
//! pass will bridge them so `ctx.luau.exec(...)` runs against the
//! same `Lua` state app `main.luau` bodies ran in; today they're
//! distinct seams.

use serde_json::Value;

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

/// What every Luau-capable host implements. One method, one shape.
///
/// **Not `Send + Sync`** — the `mlua::Lua` state behind
/// [`MluaLuauHost`] is single-threaded (`Rc<Lua>`), and the shell
/// itself runs single-threaded by construction
/// (`type Shell = Rc<RefCell<ShellInner>>`). Adding the bound would
/// force [`MluaLuauHost`] to wrap the runtime in a `Mutex` for no
/// gain.
pub trait LuauHost {
    /// Run `script` with `args` as a JSON object. Returns whatever
    /// the script's last expression evaluates to, encoded back as
    /// `serde_json::Value`. Errors surface as `Err(String)` —
    /// services wrap them into toasts.
    fn exec(&mut self, script: &str, args: &Value) -> Result<Value, String>;
}

/// Always-available fallback. Returns `Null` and records the call —
/// tests assert on the recorded calls without needing mlua linked.
#[derive(Default)]
pub struct NoopLuauHost {
    pub calls: Vec<(String, Value)>,
}

impl LuauHost for NoopLuauHost {
    fn exec(&mut self, script: &str, args: &Value) -> Result<Value, String> {
        self.calls.push((script.to_string(), args.clone()));
        Ok(Value::Null)
    }
}

/// Real `mlua`-backed `LuauHost` that delegates to a shared
/// [`prism_core::luau_runtime::LuauRuntime`]. Closes D2 of
/// `docs/dev/ui-migration-followups.md`: `ctx.luau.exec(...)` now
/// runs against the **same** persistent Lua state that app
/// `[entry] script` bodies ran in at boot, so one-shot scripts can
/// reference module-level locals registered earlier.
///
/// Lives under `feature = "native"` because the persistent runtime
/// pulls in `mlua` (vendored Luau), which doesn't build for
/// `wasm32-unknown-unknown` today. Web targets keep
/// [`NoopLuauHost`] until a wasm-friendly Lua runtime lands.
#[cfg(feature = "native")]
pub struct MluaLuauHost {
    rt: std::rc::Rc<prism_core::luau_runtime::LuauRuntime>,
}

#[cfg(feature = "native")]
impl MluaLuauHost {
    pub fn new(rt: std::rc::Rc<prism_core::luau_runtime::LuauRuntime>) -> Self {
        Self { rt }
    }
}

#[cfg(feature = "native")]
impl LuauHost for MluaLuauHost {
    fn exec(&mut self, script: &str, args: &Value) -> Result<Value, String> {
        self.rt.exec(script, args)
    }
}

/// Pure command surface. The actual runtime lives on `MutCtx::luau`,
/// so this service is a one-page declaration of *what* the user
/// can ask Luau to do.
#[derive(Default)]
pub struct LuauService;

impl ShellService for LuauService {
    fn id(&self) -> &'static str {
        "luau"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            // `luau.run-selection` — execute the active code-buffer as
            // a script. The result lands as a toast.
            cmd!("luau.run-selection", "Run Luau Selection", "Tools", |ctx| {
                let script = ctx.state.canvas.code_buffer.source().to_string();
                match ctx.luau.exec(&script, &Value::Object(Default::default())) {
                    Ok(v) => ctx.state.overlay.toasts.push(crate::state::Toast {
                        title: "Luau".into(),
                        body: v.to_string(),
                        kind: crate::state::ToastKind::Info,
                    }),
                    Err(e) => ctx.state.overlay.toasts.push(crate::state::Toast {
                        title: "Luau error".into(),
                        body: e,
                        kind: crate::state::ToastKind::Error,
                    }),
                }
            }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{Clipboard, MutCtx, OsVfs, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    /// D2 — the real `MluaLuauHost` runs Lua through the shared
    /// `LuauRuntime`. Verifies `return 6 * 7` round-trips as `42`
    /// through `LuauHost::exec`. The `feature = "native"` gate keeps
    /// this test off the wasm matrix where mlua doesn't link.
    #[cfg(feature = "native")]
    #[test]
    fn mlua_host_executes_luau_returning_number() {
        let registrar: std::sync::Arc<dyn prism_core::AppRegistrar> =
            std::sync::Arc::new(crate::app_registry::ShellAppRegistrar::default());
        let rt = prism_core::luau_runtime::LuauRuntime::new_with_tokens(
            registrar,
            prism_core::design_tokens::DEFAULT_TOKENS,
            prism_core::shell_mode::ShellMode::Build,
            prism_core::shell_mode::Permission::Dev,
        )
        .expect("LuauRuntime");
        let mut host = MluaLuauHost::new(std::rc::Rc::new(rt));
        let result = host
            .exec(
                "return 6 * 7",
                &serde_json::Value::Object(Default::default()),
            )
            .expect("exec");
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn run_selection_pushes_toast_via_noop_host() {
        let mut state = AppState::default();
        state.canvas.code_buffer.load("return 1", "luau");
        let mut undo = UndoStack::default();
        let mut vfs = OsVfs;
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let reg = ServiceRegistry::with_builtins();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        assert!(reg.commands().run("luau.run-selection", &mut ctx));
        assert_eq!(state.overlay.toasts.len(), 1);
        assert_eq!(luau.calls.len(), 1);
    }
}
