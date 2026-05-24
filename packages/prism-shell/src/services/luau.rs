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

/// Browser-side `LuauHost`. Owns a JS-side `Function(name, payloadJson)`
/// pointing at the emscripten-compiled daemon (`prism-daemon` built for
/// `wasm32-unknown-unknown-emscripten`, see `packages/prism-daemon/src/wasm.rs`).
///
/// Why a sidecar wasm. `wasm32-unknown-unknown` (what the shell ships on
/// today) has no upstream `libc++`, so the vendored Luau C++ source mlua
/// pulls in won't link there. Emscripten ships one, so the daemon — and the
/// real `mlua` Luau runtime inside it — is built into its own
/// `wasm32-unknown-emscripten` module and loaded next to the shell. The
/// shell calls into it through emscripten's `cwrap`/`ccall` glue.
///
/// Envelope shape on the wire matches the C ABI in `prism-daemon`:
///
/// ```json
/// { "ok": true,  "result": <command output> }
/// { "ok": false, "error":  "<message>" }
/// ```
///
/// `JsLuauHost::exec` packages `{ "script": ..., "args": ... }` into the
/// `luau.exec` payload, calls the JS function, and unwraps the envelope.
#[cfg(feature = "web")]
pub struct JsLuauHost {
    invoker: js_sys::Function,
}

#[cfg(feature = "web")]
impl JsLuauHost {
    pub fn new(invoker: js_sys::Function) -> Self {
        Self { invoker }
    }

    /// Pure decoder for the daemon's `{ok, result|error}` envelope. Exposed
    /// for unit tests so the JSON-shape parsing can be exercised without a
    /// live JS callback.
    pub(crate) fn decode_envelope(text: &str) -> Result<Value, String> {
        let parsed: Value = serde_json::from_str(text)
            .map_err(|e| format!("daemon response is not valid JSON: {e}"))?;
        match parsed.get("ok").and_then(|v| v.as_bool()) {
            Some(true) => Ok(parsed.get("result").cloned().unwrap_or(Value::Null)),
            Some(false) => Err(parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("daemon returned ok=false with no error message")
                .to_string()),
            None => Err("daemon response missing `ok` field".to_string()),
        }
    }
}

#[cfg(feature = "web")]
impl LuauHost for JsLuauHost {
    fn exec(&mut self, script: &str, args: &Value) -> Result<Value, String> {
        let payload = serde_json::json!({
            "script": script,
            "args": args,
        });
        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| format!("failed to encode luau.exec payload: {e}"))?;
        let name_js = wasm_bindgen::JsValue::from_str("luau.exec");
        let payload_js = wasm_bindgen::JsValue::from_str(&payload_str);
        let returned = self
            .invoker
            .call2(&wasm_bindgen::JsValue::NULL, &name_js, &payload_js)
            .map_err(|e| {
                e.as_string()
                    .unwrap_or_else(|| "daemon invoker threw a non-string error".to_string())
            })?;
        let text = returned
            .as_string()
            .ok_or_else(|| "daemon invoker returned a non-string value".to_string())?;
        Self::decode_envelope(&text)
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

    /// `JsLuauHost::decode_envelope` is the pure half of the JS bridge —
    /// the half worth exercising without a live `js_sys::Function`. The
    /// real `exec` path is integration-tested in the browser via
    /// `web/index.html`'s emscripten boot.
    #[cfg(feature = "web")]
    #[test]
    fn js_luau_host_decode_envelope_unwraps_ok_result() {
        let out = super::JsLuauHost::decode_envelope(r#"{"ok":true,"result":42}"#).unwrap();
        assert_eq!(out, serde_json::json!(42));
    }

    #[cfg(feature = "web")]
    #[test]
    fn js_luau_host_decode_envelope_returns_error_string_on_failure() {
        let err = super::JsLuauHost::decode_envelope(r#"{"ok":false,"error":"boom"}"#).unwrap_err();
        assert_eq!(err, "boom");
    }

    #[cfg(feature = "web")]
    #[test]
    fn js_luau_host_decode_envelope_treats_missing_result_as_null() {
        let out = super::JsLuauHost::decode_envelope(r#"{"ok":true}"#).unwrap();
        assert_eq!(out, serde_json::Value::Null);
    }

    #[cfg(feature = "web")]
    #[test]
    fn js_luau_host_decode_envelope_rejects_non_json() {
        let err = super::JsLuauHost::decode_envelope("not json").unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
    }

    #[cfg(feature = "web")]
    #[test]
    fn js_luau_host_decode_envelope_rejects_missing_ok_field() {
        let err = super::JsLuauHost::decode_envelope(r#"{"x":1}"#).unwrap_err();
        assert!(err.contains("missing `ok`"), "{err}");
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
