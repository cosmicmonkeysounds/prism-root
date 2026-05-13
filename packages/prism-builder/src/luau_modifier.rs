//! Wave 8 of `docs/dev/composable-builder-plan.md` — Luau parity for
//! modifiers. Sibling to [`crate::luau_component`]; same shape, same
//! thread-local registry trick, same compile / replace surface. A
//! Luau-authored behaviour calls `prism.modifier { … }` from a
//! `.luau` source and gets back a registerable
//! [`ModifierBehaviour`].
//!
//! ```lua
//! prism.modifier {
//!     id = "luau:bounce",
//!     label = "Bounce",
//!     description = "Animates the child node on attach.",
//!     schema = {
//!         prism.field.text("amount", { label = "Bounce amount" }),
//!     },
//!     install_effects = function(node)
//!         local amount = node:props():signal("amount")
//!         effect(function() print(amount:read()) end)
//!     end,
//! }
//! ```
//!
//! `id`, `label`, `description`, and `schema` are required (schema
//! defaults to an empty array when absent). The optional `wrap` and
//! `install_effects` Luau functions extend the modifier's reactive
//! contract; `wrap` is reserved for the render-time wrapper hook
//! (today it's not yet wired through the lowering pipeline — the
//! Luau-side function is held in the registry as a follow-up seam
//! for §11.5 hot-reload).
//!
//! ## Architecture mirrors `luau_component`
//!
//! - [`LuauModifierRegistry`] owns a shared `mlua::Lua` and a
//!   `HashMap<String, ModifierKeys>` keyed by modifier id.
//! - [`LuauModifier`] is the [`ModifierBehaviour`] handle stored in
//!   the document's [`ModifierRegistry`]. It carries only the
//!   modifier descriptor + the registry key; the Lua state is
//!   `!Send` and lives in the registry behind a thread-local
//!   reachable from `Send + Sync` trait objects.
//! - During schema / wrap / install_effects, the
//!   [`ActiveModifierRegistry`] thread-local is consulted; missing
//!   installation falls back to identity wrap + empty schema so
//!   headless tests / SSR walks stay safe.

#![cfg(feature = "luau")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, Lua, RegistryKey, Table, Value as LuaValue};
use serde::Deserialize;

use crate::luau_component::{install_field_helpers_pub, lua_value_to_json};
use crate::modifier::{Modifier, ModifierBehaviour, ModifierId};
use crate::reactive_props::ReactiveProps;
use crate::registry::FieldSpec as BuilderFieldSpec;
use crate::signal::SignalDef;

use prism_ui_runtime::layout::Node as UiNode;

/// Descriptor of a Luau-authored modifier, parsed out of a
/// `prism.modifier { … }` table call. Held in [`LuauModifierRegistry`]
/// alongside the compiled Luau functions.
#[derive(Debug, Clone)]
pub struct LuauModifierDef {
    pub id: String,
    pub label: String,
    pub description: String,
    pub icon: Option<String>,
    pub schema: Vec<BuilderFieldSpec>,
}

/// Pending registration drained out of the `prism.modifier` global
/// helper at compile time. Mirrors the `PendingWidget` shape in
/// `luau_component`.
struct PendingModifier {
    def: LuauModifierDef,
    /// `wrap(modifier_props, child_node_json) -> child_node_json` —
    /// optional. Today's lowering pipeline doesn't invoke this yet
    /// (the render-time bridge from `prism_ui_runtime::layout::Node`
    /// to a Luau table is the §11.5 hot-reload follow-up); the key
    /// is held so the function survives across compile / replace.
    wrap_key: Option<RegistryKey>,
    /// `install_effects(node)` — optional. Run at modifier-attach
    /// time with a `LuauNode` carrying the per-NodeId
    /// [`ReactiveProps`] bag.
    install_effects_key: Option<RegistryKey>,
}

/// Owns the shared `mlua::Lua` state and the per-modifier compiled
/// Luau functions. Same isolation discipline as
/// `LuauRenderRegistry` — one instance per document scope.
pub struct LuauModifierRegistry {
    lua: Lua,
    defs: HashMap<String, LuauModifierDef>,
    wrap_fns: HashMap<String, RegistryKey>,
    install_effects_fns: HashMap<String, RegistryKey>,
    pending: Rc<RefCell<Vec<PendingModifier>>>,
}

impl Default for LuauModifierRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LuauModifierRegistry {
    pub fn new() -> Self {
        let lua = Lua::new();
        let pending: Rc<RefCell<Vec<PendingModifier>>> = Rc::new(RefCell::new(Vec::new()));
        // Best-effort install. Caller can re-run `install_global` if a
        // host wants to re-bind after their own setup.
        let _ = install_modifier_global(&lua, pending.clone());
        Self {
            lua,
            defs: HashMap::new(),
            wrap_fns: HashMap::new(),
            install_effects_fns: HashMap::new(),
            pending,
        }
    }

    /// Re-install the `prism.modifier` global on top of the existing
    /// Lua state. Mirror of [`crate::luau_component::LuauRenderRegistry::install_global`].
    pub fn install_global(&self) -> mlua::Result<()> {
        install_modifier_global(&self.lua, self.pending.clone())
    }

    /// Compile a `.luau` source file. Every `prism.modifier { … }`
    /// call in the source registers one modifier; this returns the
    /// resulting [`LuauModifier`]s in registration order.
    ///
    /// Re-running with the same id replaces the previous
    /// registration — same hot-reload contract as
    /// `LuauRenderRegistry::compile`.
    pub fn compile(&mut self, source: &str) -> mlua::Result<Vec<LuauModifier>> {
        self.pending.borrow_mut().clear();
        self.lua.load(source).exec()?;
        let drained: Vec<PendingModifier> = std::mem::take(&mut *self.pending.borrow_mut());
        let mut out = Vec::with_capacity(drained.len());
        for pm in drained {
            let id = pm.def.id.clone();
            if let Some(prev) = self.wrap_fns.remove(&id) {
                let _ = self.lua.remove_registry_value(prev);
            }
            if let Some(prev) = self.install_effects_fns.remove(&id) {
                let _ = self.lua.remove_registry_value(prev);
            }
            self.defs.insert(id.clone(), pm.def.clone());
            if let Some(k) = pm.wrap_key {
                self.wrap_fns.insert(id.clone(), k);
            }
            if let Some(k) = pm.install_effects_key {
                self.install_effects_fns.insert(id.clone(), k);
            }
            out.push(LuauModifier { def: pm.def });
        }
        Ok(out)
    }

    /// Re-compile a single modifier's source — mirror of
    /// `LuauRenderRegistry::replace`.
    pub fn replace(&mut self, modifier_id: &str, source: &str) -> mlua::Result<LuauModifier> {
        let registered = self.compile(source)?;
        registered
            .into_iter()
            .find(|m| m.def.id == modifier_id)
            .ok_or_else(|| {
                mlua::Error::external(format!(
                    "source did not register a modifier with id `{modifier_id}`"
                ))
            })
    }

    pub fn contains(&self, modifier_id: &str) -> bool {
        self.defs.contains_key(modifier_id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }

    /// Snapshot every registered modifier's descriptor. Used by
    /// [`generate_modifier_type_stubs`] to emit a `.d.luau` file
    /// listing the Luau-authored modifiers.
    pub fn defs(&self) -> impl Iterator<Item = &LuauModifierDef> {
        self.defs.values()
    }

    /// Run the `install_effects(node)` hook for a freshly-attached
    /// Luau modifier. Surfaces any Luau error verbatim. Falls
    /// through cleanly when no hook is registered or the modifier
    /// id is unknown.
    pub fn invoke_install_effects(
        &self,
        modifier_id: &str,
        props: &ReactiveProps,
    ) -> mlua::Result<()> {
        let Some(key) = self.install_effects_fns.get(modifier_id) else {
            return Ok(());
        };
        let fun: Function = self.lua.registry_value(key)?;
        fun.call::<()>(props.clone())?;
        Ok(())
    }
}

/// One Luau-defined modifier. Stored in the document's
/// [`crate::ModifierRegistry`] alongside Rust-defined behaviours.
#[derive(Debug, Clone)]
pub struct LuauModifier {
    def: LuauModifierDef,
}

impl LuauModifier {
    pub fn new(def: LuauModifierDef) -> Self {
        Self { def }
    }

    pub fn def(&self) -> &LuauModifierDef {
        &self.def
    }
}

impl ModifierBehaviour for LuauModifier {
    fn id(&self) -> ModifierId {
        std::borrow::Cow::Owned(self.def.id.clone())
    }
    fn label(&self) -> &str {
        &self.def.label
    }
    fn icon(&self) -> Option<&str> {
        self.def.icon.as_deref()
    }
    fn description(&self) -> &str {
        &self.def.description
    }
    fn schema(&self) -> Vec<BuilderFieldSpec> {
        self.def.schema.clone()
    }
    fn wrap(&self, _modifier: &Modifier, child: UiNode) -> UiNode {
        // Render-time wrap is deferred — see module header. The
        // identity default keeps the render fold safe when a
        // Luau-authored modifier ships with only schema + install
        // effects (the common case).
        child
    }
    fn signals(&self) -> Vec<SignalDef> {
        Vec::new()
    }
}

// ── prism.modifier global helper ────────────────────────────────────

fn install_modifier_global(
    lua: &Lua,
    pending: Rc<RefCell<Vec<PendingModifier>>>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let existing: LuaValue = globals.get("prism").unwrap_or(LuaValue::Nil);
    let wrapper = lua.create_table()?;
    if !matches!(existing, LuaValue::Nil) {
        let mt = lua.create_table()?;
        mt.set("__index", existing)?;
        wrapper.set_metatable(Some(mt));
    }

    let modifier_fn = {
        let pending = pending.clone();
        let lua_clone = lua.clone();
        lua.create_function(move |_, table: Table| {
            let def = parse_modifier_def(&table)?;
            let wrap_key = match table.get::<LuaValue>("wrap") {
                Ok(LuaValue::Function(f)) => Some(lua_clone.create_registry_value(f)?),
                _ => None,
            };
            let install_effects_key = match table.get::<LuaValue>("install_effects") {
                Ok(LuaValue::Function(f)) => Some(lua_clone.create_registry_value(f)?),
                _ => None,
            };
            pending.borrow_mut().push(PendingModifier {
                def,
                wrap_key,
                install_effects_key,
            });
            Ok(())
        })?
    };
    wrapper.set("modifier", modifier_fn)?;
    install_field_helpers_pub(lua, &wrapper)?;
    globals.set("prism", wrapper)?;
    Ok(())
}

fn parse_modifier_def(table: &Table) -> mlua::Result<LuauModifierDef> {
    let id: String = table
        .get("id")
        .map_err(|_| mlua::Error::external("prism.modifier: `id` is required"))?;
    let label: String = table.get("label").unwrap_or_else(|_| id.clone());
    let description: String = table.get("description").unwrap_or_default();
    let icon: Option<String> = table.get("icon").ok();
    let schema: Vec<BuilderFieldSpec> = parse_schema(table)?;
    Ok(LuauModifierDef {
        id,
        label,
        description,
        icon,
        schema,
    })
}

fn parse_schema(table: &Table) -> mlua::Result<Vec<BuilderFieldSpec>> {
    let val: LuaValue = table.get("schema").unwrap_or(LuaValue::Nil);
    let arr = match val {
        LuaValue::Table(t) => t,
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    let len = arr.raw_len();
    for i in 1..=len {
        let v: LuaValue = arr.raw_get(i)?;
        let json = lua_value_to_json(&v)?;
        if let Ok(spec) = BuilderFieldSpec::deserialize(json) {
            out.push(spec);
        }
    }
    Ok(out)
}

// ── Thread-local registry plumbing ──────────────────────────────────

thread_local! {
    static ACTIVE_MODIFIER_REGISTRY: RefCell<Option<*const RefCell<LuauModifierRegistry>>> =
        const { RefCell::new(None) };
}

/// RAII guard that installs a [`LuauModifierRegistry`] into the
/// thread-local slot for the duration of a render / mutator pass —
/// mirror of [`crate::luau_component::ActiveRegistry`].
pub struct ActiveModifierRegistry<'a> {
    _marker: std::marker::PhantomData<&'a RefCell<LuauModifierRegistry>>,
}

impl<'a> ActiveModifierRegistry<'a> {
    pub fn install(registry: &'a RefCell<LuauModifierRegistry>) -> Self {
        ACTIVE_MODIFIER_REGISTRY.with(|slot| {
            *slot.borrow_mut() = Some(registry as *const _);
        });
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'a> Drop for ActiveModifierRegistry<'a> {
    fn drop(&mut self) {
        ACTIVE_MODIFIER_REGISTRY.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

/// Wave 8.3 — emit `.d.luau` type stubs for every Luau-authored
/// modifier in `registry`. Mirrors
/// [`crate::signal::generate_signal_type_stubs`] for components.
///
/// Format: one `--- @class Modifier_<Id>` block per modifier, with
/// schema fields surfaced as table fields. Lands as
/// `modifiers.d.luau` next to `signals.d.luau` in the project's
/// type-stub directory.
pub fn generate_modifier_type_stubs(registry: &LuauModifierRegistry) -> String {
    let mut out = String::new();
    out.push_str(
        "--- @meta\n\
         --- Auto-generated by prism_builder::luau_modifier::generate_modifier_type_stubs.\n\
         --- Do not edit by hand.\n\n",
    );
    let mut defs: Vec<&LuauModifierDef> = registry.defs().collect();
    defs.sort_by(|a, b| a.id.cmp(&b.id));
    for def in defs {
        out.push_str(&format!("--- @class Modifier_{}\n", sanitize_id(&def.id)));
        if !def.description.is_empty() {
            for line in def.description.lines() {
                out.push_str(&format!("--- {line}\n"));
            }
        }
        for f in &def.schema {
            out.push_str(&format!(
                "--- @field {} {}\n",
                f.key,
                field_kind_to_luau_type(&f.kind)
            ));
        }
        out.push('\n');
    }
    out
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn field_kind_to_luau_type(kind: &crate::registry::FieldKind) -> &'static str {
    use crate::registry::FieldKind;
    match kind {
        FieldKind::Text | FieldKind::TextArea | FieldKind::Color | FieldKind::Date => "string",
        FieldKind::File(_) => "string",
        FieldKind::Number(_) | FieldKind::Integer(_) => "number",
        FieldKind::Boolean => "boolean",
        FieldKind::Select(_) => "string",
        _ => "any",
    }
}

/// Wave 8.4 — first-class entry point for registering a Luau
/// modifier from a script source string. Returns the freshly-built
/// [`LuauModifier`]s in source order. The caller decides where to
/// install them (typically into a document's
/// [`crate::ModifierRegistry`]).
pub fn register_modifier_from_luau(
    registry: &mut LuauModifierRegistry,
    source: &str,
) -> mlua::Result<Vec<LuauModifier>> {
    registry.compile(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::ModifierRegistry;
    use serde_json::{json, Value};
    use std::sync::Arc;

    #[test]
    fn prism_modifier_global_registers_an_entry_with_schema() {
        let mut reg = LuauModifierRegistry::new();
        let mods = reg
            .compile(
                r#"
                prism.modifier {
                    id = "luau:tooltip",
                    label = "Tooltip",
                    description = "Show a tooltip on hover.",
                    schema = {
                        prism.field.text("text", { label = "Tooltip text" }),
                    },
                }
                "#,
            )
            .expect("compile");
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].def.id, "luau:tooltip");
        assert_eq!(mods[0].def.label, "Tooltip");
        assert_eq!(mods[0].def.schema.len(), 1);
        assert_eq!(mods[0].def.schema[0].key, "text");
    }

    #[test]
    fn luau_modifier_can_be_attached_to_a_modifier_registry() {
        let mut luau_reg = LuauModifierRegistry::new();
        let mods = luau_reg
            .compile(
                r#"
                prism.modifier { id = "luau:bounce", label = "Bounce" }
                "#,
            )
            .unwrap();
        let mut reg = ModifierRegistry::new();
        reg.register(Arc::new(mods.into_iter().next().unwrap()))
            .expect("register");
        assert!(reg.contains("luau:bounce"));
        let descriptor = reg.descriptor("luau:bounce").unwrap();
        assert_eq!(descriptor.label, "Bounce");
    }

    #[test]
    fn install_effects_runs_against_reactive_props() {
        let mut reg = LuauModifierRegistry::new();
        // Install a global Lua-side `effect` shim that captures the
        // signal's snapshot — the real reactive `effect` lives in
        // the host's daemon module, but for the test we only care
        // that the hook is invoked with a working `props` userdata.
        reg.lua
            .globals()
            .set(
                "set_captured",
                reg.lua
                    .create_function(|lua, v: mlua::Value| {
                        let json: Value = mlua::LuaSerdeExt::from_value(lua, v)?;
                        lua.set_named_registry_value("captured", json.to_string())?;
                        Ok(())
                    })
                    .unwrap(),
            )
            .unwrap();
        reg.compile(
            r#"
            prism.modifier {
                id = "luau:capture",
                label = "Capture",
                install_effects = function(props)
                    set_captured(props:read("amount"))
                end,
            }
            "#,
        )
        .unwrap();
        let props = ReactiveProps::new(json!({"amount": 7}));
        reg.invoke_install_effects("luau:capture", &props).unwrap();
        let captured: String = reg.lua.named_registry_value("captured").unwrap();
        assert_eq!(captured, "7");
    }

    #[test]
    fn install_effects_is_a_noop_when_hook_absent() {
        let mut reg = LuauModifierRegistry::new();
        reg.compile(r#"prism.modifier { id = "luau:nohook", label = "No" }"#)
            .unwrap();
        let props = ReactiveProps::default();
        reg.invoke_install_effects("luau:nohook", &props).unwrap();
    }

    #[test]
    fn install_effects_is_a_noop_when_id_unknown() {
        let reg = LuauModifierRegistry::new();
        let props = ReactiveProps::default();
        reg.invoke_install_effects("luau:does-not-exist", &props)
            .unwrap();
    }

    #[test]
    fn replace_swaps_a_single_modifier_in_place() {
        let mut reg = LuauModifierRegistry::new();
        reg.compile(r#"prism.modifier { id = "luau:m", label = "A" }"#)
            .unwrap();
        let replaced = reg
            .replace("luau:m", r#"prism.modifier { id = "luau:m", label = "B" }"#)
            .unwrap();
        assert_eq!(replaced.def.label, "B");
    }

    #[test]
    fn replace_errors_when_source_does_not_register_target_id() {
        let mut reg = LuauModifierRegistry::new();
        reg.compile(r#"prism.modifier { id = "luau:m", label = "A" }"#)
            .unwrap();
        let err = reg
            .replace(
                "luau:m",
                r#"prism.modifier { id = "luau:other", label = "X" }"#,
            )
            .unwrap_err();
        assert!(err.to_string().contains("did not register"));
    }

    #[test]
    fn generate_modifier_type_stubs_emits_class_blocks_in_id_order() {
        let mut reg = LuauModifierRegistry::new();
        reg.compile(
            r#"
            prism.modifier {
                id = "luau:b",
                label = "B",
                description = "Second.",
                schema = { prism.field.text("text") },
            }
            prism.modifier {
                id = "luau:a",
                label = "A",
                description = "First.",
                schema = { prism.field.boolean("on") },
            }
            "#,
        )
        .unwrap();
        let stubs = generate_modifier_type_stubs(&reg);
        // Sorted alphabetically by id, so `luau:a` precedes `luau:b`.
        let a_idx = stubs.find("Modifier_luau_a").expect("luau:a present");
        let b_idx = stubs.find("Modifier_luau_b").expect("luau:b present");
        assert!(a_idx < b_idx);
        assert!(stubs.contains("@field on boolean"));
        assert!(stubs.contains("@field text string"));
    }

    #[test]
    fn register_modifier_from_luau_is_a_thin_compile_alias() {
        let mut reg = LuauModifierRegistry::new();
        let registered = register_modifier_from_luau(
            &mut reg,
            r#"prism.modifier { id = "luau:m", label = "M" }"#,
        )
        .unwrap();
        assert_eq!(registered.len(), 1);
        assert!(reg.contains("luau:m"));
    }

    #[test]
    fn props_userdata_round_trips_a_signal_value_through_luau() {
        // Confirms `ReactiveProps`'s mlua UserData impl is usable
        // independently of the modifier registry. The `signal`
        // method returns the canonical `Signal<Value>` userdata so
        // the full reactive vocabulary is available without a
        // parallel wrapper type.
        let lua = Lua::new();
        let props = ReactiveProps::new(json!({"count": 3}));
        lua.globals().set("props", props.clone()).unwrap();
        let result: i64 = lua
            .load("return props:signal('count'):peek()")
            .eval()
            .unwrap();
        assert_eq!(result, 3);
        // And `write` propagates back to the canonical store.
        lua.load("props:write('count', 11)").exec().unwrap();
        assert_eq!(props.get("count"), json!(11));
    }

    #[test]
    fn props_signal_read_returns_current_value_through_luau() {
        // Mirror of `read` semantics — `:signal(k):read()` returns
        // the current value. Subscribing semantics under a
        // reactive context are covered by
        // `prism_core::luau_reactive::tests::signal_read_returns_current_value_to_luau`.
        let lua = Lua::new();
        let props = ReactiveProps::new(json!({"x": 1}));
        lua.globals().set("props", props.clone()).unwrap();
        let v: i64 = lua.load("return props:signal('x'):read()").eval().unwrap();
        assert_eq!(v, 1);
    }
}
