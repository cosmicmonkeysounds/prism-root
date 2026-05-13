//! Persistent Luau runtime — owns a long-lived `mlua::Lua` state plus
//! a [`LuauCallbackStore`] of retained closures so the
//! `register_component({render=fn})` / `register_service({on_event=fn})`
//! Lua surface composes with later host-side dispatch.
//!
//! Daemon-side `luau.exec` creates a fresh `Lua` per call — fine for
//! one-shot scripts but unworkable for boot scripts that register
//! components/services whose bodies the shell calls back into. This
//! module is the long-lived counterpart: build one runtime at
//! `Shell::new`, run every app's `main.luau` against it, then dispatch
//! every retained closure from the live render / event loops.
//!
//! The runtime is single-threaded by construction (`Rc<Lua>`); the
//! shell render path is single-threaded too. Cross-thread script
//! execution belongs to the daemon's separate `LuauHost` and is out of
//! scope here.
//!
//! Closes the "persistent Luau system" residual follow-up in
//! `docs/dev/dsl-self-bootstrap.md`.

use std::rc::Rc;
use std::sync::Arc;

use mlua::{Function, Lua, LuaSerdeExt, Value};
use serde_json::Value as JsonValue;

use crate::app_registry::AppRegistrar;
use crate::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use crate::luau_bindings::{LuauCallbackStore, RegistrarHandle};
use crate::shell_mode::{Permission, ShellMode};

/// What every Luau-script-author returns from a `render(props, children)`
/// body, translated by the runtime into a tree the shell can lower
/// directly. Three shapes:
///
/// * `Text(s)` — a leaf text run. Hosts wrap this in their own text
///   primitive (the shell's `LuauComponentBlock` lowers it under a
///   semantic `<span>`).
/// * `Element { tag, attrs, children }` — a container node. `tag` is
///   the semantic HTML tag the host emits (`div`, `section`, …).
///   `attrs` is the flat attribute list (e.g. `data-role="card"`).
///   `children` is the recursive body.
/// * `None` — the script returned `nil`; the host renders its
///   placeholder body unchanged.
///
/// The shape is deliberately small: it's enough to render a useful
/// surface without committing the runtime to a full virtual-DOM
/// diff — hosts that need more reach for them as raw `serde_json`.
#[derive(Clone, Debug, PartialEq)]
pub enum VirtualNode {
    Text(String),
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<VirtualNode>,
    },
}

impl VirtualNode {
    /// Convenience builder for a one-text-child element.
    pub fn elem_with_text(tag: impl Into<String>, text: impl Into<String>) -> Self {
        VirtualNode::Element {
            tag: tag.into(),
            attrs: Vec::new(),
            children: vec![VirtualNode::Text(text.into())],
        }
    }
}

/// What a `LuauScriptedService::on_event` body returns. Trimmed shape
/// of `prism_shell::services::EventOutcome` — the shell maps `Handled`
/// to `EventOutcome::Handled`, everything else to `Pass`. We pin the
/// shape here rather than re-exporting the shell-side enum because
/// `prism-core` is the leaf — the dispatch result has to travel down,
/// not up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LuauEventOutcome {
    Handled,
    Pass,
}

/// Owns a persistent `Lua` state plus the [`LuauCallbackStore`]
/// retained closures land in. Cloneable via `Rc`-internal: every clone
/// shares the same Lua state + retention tables, so the shell can hand
/// the runtime into every Luau-backed block / service without
/// re-installing globals.
#[derive(Clone)]
pub struct LuauRuntime {
    lua: Rc<Lua>,
    callbacks: LuauCallbackStore,
}

impl LuauRuntime {
    /// Construct a fresh runtime. Installs:
    ///
    /// * `prism.app` — [`RegistrarHandle`] bound to the supplied
    ///   `registrar` plus the runtime's [`LuauCallbackStore`]. Scripts
    ///   call `prism.app:register_panel(...)` /
    ///   `prism.app:register_component({render = fn})` etc.
    /// * `prism.element(tag, attrs?, children?)` — helper to build
    ///   element-shaped tables. Equivalent to
    ///   `{tag = ..., attrs = ..., children = ...}` literally; we ship
    ///   it as a shortcut so authoring stays terse.
    ///
    /// More globals (design tokens, shell-mode, object handles) can be
    /// layered on by the host *after* `new` — every `RegistryKey` etc.
    /// only depends on the Lua state being alive, not on the globals
    /// being final.
    pub fn new(registrar: Arc<dyn AppRegistrar>) -> Result<Self, String> {
        Self::new_with_tokens(registrar, DEFAULT_TOKENS, ShellMode::Build, Permission::Dev)
    }

    /// Variant that lets the host supply non-default design tokens /
    /// shell-mode / permission tags. The shell calls this with its
    /// live boot configuration so scripts see the same theme + mode
    /// the chrome renders against.
    pub fn new_with_tokens(
        registrar: Arc<dyn AppRegistrar>,
        tokens: DesignTokens,
        shell_mode: ShellMode,
        permission: Permission,
    ) -> Result<Self, String> {
        let lua = Lua::new();
        let callbacks = LuauCallbackStore::new();
        let app = RegistrarHandle::new(registrar).with_callbacks(callbacks.clone());

        let prism_tbl = lua
            .create_table()
            .map_err(|e| format!("create prism table: {e}"))?;
        prism_tbl
            .set("app", app)
            .map_err(|e| format!("install prism.app: {e}"))?;
        prism_tbl
            .set("tokens", tokens)
            .map_err(|e| format!("install prism.tokens: {e}"))?;
        prism_tbl
            .set("shell_mode", shell_mode)
            .map_err(|e| format!("install prism.shell_mode: {e}"))?;
        prism_tbl
            .set("permission", permission)
            .map_err(|e| format!("install prism.permission: {e}"))?;

        // `prism.element` builder — purely syntactic sugar around the
        // virtual-node shape. Wraps positional args into the
        // `{tag, attrs, children}` literal the runtime already knows
        // how to walk.
        let element_fn = lua
            .create_function(
                |lua, (tag, attrs, children): (String, Option<mlua::Table>, Option<mlua::Table>)| {
                    let t = lua.create_table()?;
                    t.set("tag", tag)?;
                    if let Some(a) = attrs {
                        t.set("attrs", a)?;
                    }
                    if let Some(c) = children {
                        t.set("children", c)?;
                    }
                    Ok(t)
                },
            )
            .map_err(|e| format!("create prism.element: {e}"))?;
        prism_tbl
            .set("element", element_fn)
            .map_err(|e| format!("install prism.element: {e}"))?;

        lua.globals()
            .set("prism", prism_tbl)
            .map_err(|e| format!("install prism global: {e}"))?;

        Ok(Self {
            lua: Rc::new(lua),
            callbacks,
        })
    }

    /// Direct access to the inner Lua state — used by hosts that need
    /// to layer additional globals (design tokens, object handles)
    /// after construction. Carry through `Rc::clone` so the runtime
    /// retains ownership.
    pub fn lua(&self) -> &Rc<Lua> {
        &self.lua
    }

    pub fn callbacks(&self) -> &LuauCallbackStore {
        &self.callbacks
    }

    /// Run `source` as the named chunk `name`. Surface errors as
    /// `String` so callers can log + skip gracefully — a misbehaving
    /// app shouldn't break the rest of the shell boot.
    pub fn load_script(&self, source: &str, name: &str) -> Result<(), String> {
        self.lua
            .load(source)
            .set_name(name)
            .exec()
            .map_err(|e| format!("{name}: {e}"))
    }

    /// Invoke the retained `render` fn under `key`. `props` is handed
    /// to the script as a Lua table (via the serde bridge); `children`
    /// is passed through verbatim as a JSON array. Returns whatever
    /// the script produced, translated into a [`VirtualNode`]. `None`
    /// means no callback was retained for `key` — the caller falls
    /// through to its placeholder body. `Err(_)` means dispatch ran but
    /// failed (script error, malformed return shape).
    pub fn call_render(
        &self,
        key: &str,
        props: &JsonValue,
        children: &[JsonValue],
    ) -> Option<Result<VirtualNode, String>> {
        // `with_render` is short-circuit-friendly — `None` flows back
        // when nothing's retained for the key, so we never touch Lua.
        self.callbacks.with_render(key, |rk| {
            let f: Function = self
                .lua
                .registry_value(rk)
                .map_err(|e| format!("resolve render fn `{key}`: {e}"))?;
            let props_lua = self
                .lua
                .to_value(props)
                .map_err(|e| format!("serialize props for `{key}`: {e}"))?;
            let children_lua = self
                .lua
                .to_value(children)
                .map_err(|e| format!("serialize children for `{key}`: {e}"))?;
            let ret: Value = f
                .call((props_lua, children_lua))
                .map_err(|e| format!("call render `{key}`: {e}"))?;
            value_to_virtual_node(&self.lua, ret)
                .map_err(|e| format!("decode render result for `{key}`: {e}"))
        })
    }

    /// Invoke the retained `on_event` fn under `key`. The event is
    /// passed as a Lua table. Return shape: any truthy string
    /// containing "Handled" (case-insensitive) or boolean `true` →
    /// [`LuauEventOutcome::Handled`]; anything else (nil, false, …) →
    /// [`LuauEventOutcome::Pass`]. `None` when no callback was
    /// retained.
    pub fn call_on_event(
        &self,
        key: &str,
        event: &JsonValue,
    ) -> Option<Result<LuauEventOutcome, String>> {
        self.callbacks.with_on_event(key, |rk| {
            let f: Function = self
                .lua
                .registry_value(rk)
                .map_err(|e| format!("resolve on_event fn `{key}`: {e}"))?;
            let event_lua = self
                .lua
                .to_value(event)
                .map_err(|e| format!("serialize event for `{key}`: {e}"))?;
            let ret: Value = f
                .call(event_lua)
                .map_err(|e| format!("call on_event `{key}`: {e}"))?;
            Ok(decode_event_outcome(&ret))
        })
    }

    /// Run `script` against the persistent state, surfacing the
    /// returned value as JSON. Mirrors the daemon's `luau.exec`
    /// contract — the difference is that this state survives between
    /// calls, so module-level `local`s authored in one `exec` survive
    /// to the next. Useful for ad-hoc REPL-style scripting against
    /// the same state that drove app boot.
    pub fn exec(&self, script: &str, args: &JsonValue) -> Result<JsonValue, String> {
        if let Some(obj) = args.as_object() {
            let globals = self.lua.globals();
            for (k, v) in obj {
                let lua_v = self
                    .lua
                    .to_value(v)
                    .map_err(|e| format!("serialize arg `{k}`: {e}"))?;
                globals
                    .set(k.as_str(), lua_v)
                    .map_err(|e| format!("install arg `{k}`: {e}"))?;
            }
        }
        let ret: Value = self
            .lua
            .load(script)
            .set_name("exec")
            .eval()
            .map_err(|e| format!("exec: {e}"))?;
        self.lua
            .from_value::<JsonValue>(ret)
            .map_err(|e| format!("decode exec result: {e}"))
    }
}

// ───── value → VirtualNode translation ────────────────────────────

fn value_to_virtual_node(lua: &Lua, v: Value) -> mlua::Result<VirtualNode> {
    match v {
        // Scalar: render as a text leaf.
        Value::Nil => Ok(VirtualNode::Text(String::new())),
        Value::Boolean(b) => Ok(VirtualNode::Text(b.to_string())),
        Value::Integer(i) => Ok(VirtualNode::Text(i.to_string())),
        Value::Number(n) => Ok(VirtualNode::Text(n.to_string())),
        Value::String(s) => Ok(VirtualNode::Text(s.to_str()?.to_string())),
        // Element table: walk `tag`, `attrs`, `children`.
        Value::Table(t) => table_to_virtual_node(lua, t),
        // Function / Thread / UserData / LightUserData / Error / Other:
        // surface a typed error so authors learn quickly.
        other => Err(mlua::Error::external(format!(
            "render returned unsupported value: {other:?}"
        ))),
    }
}

fn table_to_virtual_node(lua: &Lua, t: mlua::Table) -> mlua::Result<VirtualNode> {
    let tag: Option<String> = t.get::<Option<String>>("tag")?;
    let Some(tag) = tag else {
        // No `tag` — treat the table as a list of children (`#t` length)
        // wrapped in a fragment-like default element. This is the
        // shape `{{...}, {...}}` produced when a script returns a
        // sequence of nodes without a wrapping element.
        let mut children = Vec::new();
        for pair in t.sequence_values::<Value>() {
            children.push(value_to_virtual_node(lua, pair?)?);
        }
        return Ok(VirtualNode::Element {
            tag: "div".into(),
            attrs: Vec::new(),
            children,
        });
    };
    let attrs = if let Some(a) = t.get::<Option<mlua::Table>>("attrs")? {
        let mut out = Vec::new();
        for pair in a.pairs::<String, Value>() {
            let (k, v) = pair?;
            let v_str = match v {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(i) => i.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                Value::Nil => continue,
                other => {
                    return Err(mlua::Error::external(format!(
                        "attr `{k}` has unsupported value: {other:?}"
                    )));
                }
            };
            out.push((k, v_str));
        }
        // Stable ordering — Lua iteration order is unspecified, so we
        // sort lexicographically so re-renders don't churn dirty.
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    } else {
        Vec::new()
    };
    let mut children = Vec::new();
    // Children: explicit `children` array if present, otherwise the
    // table's own sequence values (so `{tag="div", "hello", "world"}`
    // works). Inline `text` sugar appends a trailing text run.
    if let Some(c) = t.get::<Option<mlua::Table>>("children")? {
        for pair in c.sequence_values::<Value>() {
            children.push(value_to_virtual_node(lua, pair?)?);
        }
    } else {
        for pair in t.clone().sequence_values::<Value>() {
            children.push(value_to_virtual_node(lua, pair?)?);
        }
    }
    if let Some(text) = t.get::<Option<String>>("text")? {
        children.push(VirtualNode::Text(text));
    }
    Ok(VirtualNode::Element {
        tag,
        attrs,
        children,
    })
}

fn decode_event_outcome(v: &Value) -> LuauEventOutcome {
    match v {
        Value::Boolean(true) => LuauEventOutcome::Handled,
        Value::String(s) => {
            // `to_str` fails on non-UTF8; treat as `Pass` in that case
            // — authors should return a real string, but we don't want
            // a malformed return to abort the event router.
            match s.to_str() {
                Ok(s) if s.eq_ignore_ascii_case("handled") => LuauEventOutcome::Handled,
                _ => LuauEventOutcome::Pass,
            }
        }
        _ => LuauEventOutcome::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_registry::{NoopAppRegistrar, PanelRegistration};
    use serde_json::json;
    use std::sync::Mutex;

    /// Recording registrar so tests can assert script-driven
    /// registrations land in the host's bookkeeping.
    #[derive(Default)]
    struct Rec {
        panels: Mutex<Vec<PanelRegistration>>,
    }

    impl AppRegistrar for Rec {
        fn register_panel(
            &self,
            p: PanelRegistration,
        ) -> Result<(), crate::app_registry::RegistrationError> {
            self.panels.lock().unwrap().push(p);
            Ok(())
        }
        fn register_component(
            &self,
            _c: crate::app_registry::ComponentRegistration,
        ) -> Result<(), crate::app_registry::RegistrationError> {
            Ok(())
        }
        fn register_service(
            &self,
            _s: crate::app_registry::ServiceRegistration,
        ) -> Result<(), crate::app_registry::RegistrationError> {
            Ok(())
        }
    }

    #[test]
    fn new_installs_prism_app_and_element_helpers() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        // `prism.app` is the userdata; `prism.element` is a function.
        runtime
            .load_script(
                r#"
                    assert(prism ~= nil, "prism global missing")
                    assert(prism.app ~= nil, "prism.app missing")
                    assert(type(prism.element) == "function", "prism.element missing")
                "#,
                "asserts",
            )
            .unwrap();
    }

    #[test]
    fn new_exposes_design_tokens_shell_mode_permission() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        // Default `ShellMode::Build` / `Permission::Dev` / default
        // tokens — the runtime mirrors the daemon's PrismContext
        // shape so scripts authored against either run unchanged.
        let r: i64 = runtime
            .exec("return prism.tokens.colors.accent.r", &json!({}))
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(r, 110);
        let mode: String = runtime
            .exec("return prism.shell_mode", &json!({}))
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(mode, "Build");
        let perm: String = runtime
            .exec("return prism.permission", &json!({}))
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(perm, "Dev");
    }

    #[test]
    fn load_script_persistent_state_survives_calls() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                "_G.counter = 0; _G.bump = function() counter = counter + 1 end",
                "boot",
            )
            .unwrap();
        runtime
            .load_script("bump(); bump(); bump()", "tick")
            .unwrap();
        let n = runtime.exec("return counter", &json!({})).unwrap();
        assert_eq!(n.as_i64(), Some(3));
    }

    #[test]
    fn app_registration_from_boot_script_flows_through() {
        let rec = Arc::new(Rec::default());
        let runtime = LuauRuntime::new(rec.clone()).unwrap();
        runtime
            .load_script(
                r#"prism.app:register_panel({ id = "from.script", label = "From Script" })"#,
                "register",
            )
            .unwrap();
        assert_eq!(rec.panels.lock().unwrap().len(), 1);
        assert_eq!(rec.panels.lock().unwrap()[0].id, "from.script");
    }

    #[test]
    fn call_render_returns_none_when_no_callback_retained() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        assert!(runtime
            .call_render("nothing.here", &json!({}), &[])
            .is_none());
    }

    #[test]
    fn register_component_then_dispatch_round_trips_through_runtime() {
        // The persistent-Luau end-to-end contract: a script registers
        // a component with a `render` body, and a later
        // `call_render(key, ...)` invocation finds the retained fn and
        // returns its result.
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.card",
                        render = function(props, _children)
                            return prism.element("section",
                                { ["data-role"] = "card" },
                                { props.name })
                        end,
                    })
                "#,
                "card.luau",
            )
            .unwrap();
        let result = runtime
            .call_render("my.card.render", &json!({ "name": "Hi" }), &[])
            .expect("callback retained")
            .expect("dispatch succeeds");
        let VirtualNode::Element {
            tag,
            attrs,
            children,
        } = result
        else {
            panic!("expected element, got: {result:?}");
        };
        assert_eq!(tag, "section");
        assert_eq!(
            attrs.as_slice(),
            &[("data-role".to_string(), "card".to_string())]
        );
        assert_eq!(children.len(), 1);
        assert!(matches!(&children[0], VirtualNode::Text(t) if t == "Hi"));
    }

    #[test]
    fn render_string_return_lowers_as_text_leaf() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.text",
                        render = function(_p, _c) return "hello world" end,
                    })
                "#,
                "text.luau",
            )
            .unwrap();
        let result = runtime
            .call_render("my.text.render", &json!({}), &[])
            .unwrap()
            .unwrap();
        assert!(matches!(result, VirtualNode::Text(ref s) if s == "hello world"));
    }

    #[test]
    fn render_nil_return_lowers_as_empty_text() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.empty",
                        render = function() return nil end,
                    })
                "#,
                "empty.luau",
            )
            .unwrap();
        let result = runtime
            .call_render("my.empty.render", &json!({}), &[])
            .unwrap()
            .unwrap();
        assert!(matches!(result, VirtualNode::Text(ref s) if s.is_empty()));
    }

    #[test]
    fn render_error_surfaces_as_err_not_panic() {
        // Misbehaving script body — accessing a non-existent global
        // raises in Luau. The runtime must catch it as a typed `Err`
        // so the shell can fall through to its placeholder body
        // rather than crashing the render walk.
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.broken",
                        render = function() return does_not_exist:method() end,
                    })
                "#,
                "broken.luau",
            )
            .unwrap();
        let result = runtime
            .call_render("my.broken.render", &json!({}), &[])
            .expect("callback retained");
        assert!(result.is_err(), "expected Err, got: {result:?}");
    }

    #[test]
    fn on_event_handled_string_decodes_to_handled() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_service({
                        id = "my.svc",
                        on_event = function(_event) return "Handled" end,
                    })
                "#,
                "svc.luau",
            )
            .unwrap();
        let outcome = runtime
            .call_on_event("my.svc.on_event", &json!({ "kind": "Wheel" }))
            .unwrap()
            .unwrap();
        assert_eq!(outcome, LuauEventOutcome::Handled);
    }

    #[test]
    fn on_event_nil_return_decodes_to_pass() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_service({
                        id = "my.svc",
                        on_event = function(_event) end,
                    })
                "#,
                "svc.luau",
            )
            .unwrap();
        let outcome = runtime
            .call_on_event("my.svc.on_event", &json!({}))
            .unwrap()
            .unwrap();
        assert_eq!(outcome, LuauEventOutcome::Pass);
    }

    #[test]
    fn nested_element_children_are_translated_recursively() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.box",
                        render = function(_p, _c)
                            return prism.element("section", nil, {
                                prism.element("h1", nil, { "Title" }),
                                prism.element("p", nil, { "Body" }),
                            })
                        end,
                    })
                "#,
                "box.luau",
            )
            .unwrap();
        let result = runtime
            .call_render("my.box.render", &json!({}), &[])
            .unwrap()
            .unwrap();
        let VirtualNode::Element { tag, children, .. } = result else {
            panic!("expected element, got: {result:?}");
        };
        assert_eq!(tag, "section");
        assert_eq!(children.len(), 2);
        let VirtualNode::Element {
            tag: t0,
            children: c0,
            ..
        } = &children[0]
        else {
            panic!("expected h1");
        };
        assert_eq!(t0, "h1");
        assert!(matches!(&c0[0], VirtualNode::Text(s) if s == "Title"));
        let VirtualNode::Element { tag: t1, .. } = &children[1] else {
            panic!("expected p");
        };
        assert_eq!(t1, "p");
    }

    #[test]
    fn exec_returns_serialized_value() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        let result = runtime.exec("return x * 2", &json!({ "x": 21 })).unwrap();
        assert_eq!(result.as_i64(), Some(42));
    }

    #[test]
    fn exec_table_round_trips_as_json_object() {
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        let result = runtime
            .exec(r#"return { name = "x", count = 7 }"#, &json!({}))
            .unwrap();
        let obj = result.as_object().expect("object");
        assert_eq!(obj.get("name").unwrap().as_str(), Some("x"));
        assert_eq!(obj.get("count").unwrap().as_i64(), Some(7));
    }

    #[test]
    fn re_registering_component_replaces_render_fn() {
        // Re-registering under the same id overwrites the retained
        // closure — matches the hot-reload contract.
        let runtime = LuauRuntime::new(Arc::new(NoopAppRegistrar)).unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.x",
                        render = function() return "v1" end,
                    })
                "#,
                "v1",
            )
            .unwrap();
        runtime
            .load_script(
                r#"
                    prism.app:register_component({
                        id = "my.x",
                        render = function() return "v2" end,
                    })
                "#,
                "v2",
            )
            .unwrap();
        let result = runtime
            .call_render("my.x.render", &json!({}), &[])
            .unwrap()
            .unwrap();
        assert!(matches!(result, VirtualNode::Text(ref s) if s == "v2"));
    }
}
