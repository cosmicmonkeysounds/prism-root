//! Luau-defined widgets — Phase 6 of `docs/dev/luau-integration-plan.md`.
//!
//! Authors register a widget by calling `prism.widget { ... }` from a
//! `.luau` file. The table carries the same fields as a Rust
//! [`WidgetContribution`] plus a `render` function. The render function
//! returns a [`VirtualNode`] tree the host walks through the existing
//! [`ComponentRegistry`] / [`HtmlRegistry`] — Slint DSL is never produced
//! directly from Luau (per the plan's "node-tree intermediary" decision).
//!
//! ## Architecture
//!
//! - [`LuauRenderRegistry`] owns the shared [`mlua::Lua`] state and a
//!   `HashMap<String, RegistryKey>` of compiled render functions, keyed
//!   by component id.
//! - [`LuauComponent`] is the [`Block`]-implementing handle stored in the
//!   shell's [`ComponentRegistry`]. It carries only the contribution
//!   metadata + the component id needed to look up the render function;
//!   the `mlua::Lua` state is `!Send`, so it stays in the registry.
//! - During render, [`LuauComponent::render_slint`] /
//!   [`render_html`](LuauComponent::render_html) reach for the registry
//!   through a thread-local because the [`Component`] trait is
//!   `Send + Sync` and can't carry an `Rc`.
//! - The walker produces a [`VirtualNode`] tree, then recurses through
//!   the existing component / html registries — so a Luau-defined
//!   widget composes natively with `Card`, `Container`, etc.
//!
//! ## Hot reload
//!
//! [`LuauRenderRegistry::replace`] re-compiles a single widget's source
//! against the same `Lua` state, swaps its `RegistryKey`, and leaves
//! every other widget's compiled function intact. Module-level state
//! is discarded by design (per plan §"Hot-reload — per-component
//! re-registration").

#![cfg(feature = "luau")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use mlua::{FromLua, Function, Lua, RegistryKey, Table, Value as LuaValue};
use prism_core::widget::field::FieldSpec;
use prism_core::widget::{
    LayoutDirection, SignalSpec, TemplateNode, VariantOptionSpec, VariantSpec, WidgetCategory,
    WidgetContribution, WidgetSize, WidgetTemplate,
};
use serde_json::Value;

use crate::block::Block;
use crate::component::{ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::html::Html;
use crate::html_block::HtmlRenderContext;
use crate::registry::FieldSpec as BuilderFieldSpec;
use crate::signal::SignalDef;
use crate::slint_source::SlintEmitter;
use crate::variant::VariantAxis;

// ── VirtualNode (Phase 6b) ──────────────────────────────────────────

/// Node descriptor returned by a Luau `render` function. Same shape as
/// [`Node`] minus the persistent `id` (the host mints synthetic ids on
/// the fly so the source map stays consistent).
#[derive(Debug, Clone, Default)]
pub struct VirtualNode {
    pub component: String,
    pub props: Value,
    pub children: Vec<VirtualNode>,
}

impl VirtualNode {
    /// Promote into a real [`Node`]. Synthetic ids are minted from
    /// `next_id` so the walker can keep them unique within one render
    /// pass.
    pub fn into_node(self, next_id: &mut u64) -> Node {
        let id_n = *next_id;
        *next_id = next_id.saturating_add(1);
        Node {
            id: format!("luau-{id_n}"),
            component: self.component,
            props: self.props,
            children: self
                .children
                .into_iter()
                .map(|c| c.into_node(next_id))
                .collect(),
            ..Default::default()
        }
    }
}

impl mlua::FromLua for VirtualNode {
    fn from_lua(value: LuaValue, lua: &Lua) -> mlua::Result<Self> {
        let table: Table = match value {
            LuaValue::Table(t) => t,
            other => {
                return Err(mlua::Error::FromLuaConversionError {
                    from: other.type_name(),
                    to: "VirtualNode".into(),
                    message: Some(
                        "expected table { component = ..., props = ..., children = ... }".into(),
                    ),
                })
            }
        };
        let component: String = table.get("component")?;

        let props = match table.get::<LuaValue>("props")? {
            LuaValue::Nil => Value::Null,
            v => lua_value_to_json(&v)?,
        };

        let children: Vec<VirtualNode> = match table.get::<LuaValue>("children")? {
            LuaValue::Nil => Vec::new(),
            LuaValue::Table(t) => {
                let mut out = Vec::new();
                let len = t.raw_len();
                for i in 1..=len {
                    let v: LuaValue = t.raw_get(i)?;
                    out.push(VirtualNode::from_lua(v, lua)?);
                }
                out
            }
            other => {
                return Err(mlua::Error::FromLuaConversionError {
                    from: other.type_name(),
                    to: "Vec<VirtualNode>".into(),
                    message: Some("`children` must be an array of VirtualNode tables".into()),
                });
            }
        };
        Ok(VirtualNode {
            component,
            props,
            children,
        })
    }
}

// ── JSON ↔ Lua helpers ──────────────────────────────────────────────

fn lua_value_to_json(value: &LuaValue) -> mlua::Result<Value> {
    Ok(match value {
        LuaValue::Nil => Value::Null,
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::Number((*i).into()),
        LuaValue::Number(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        LuaValue::String(s) => Value::String(s.to_str()?.to_string()),
        LuaValue::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: LuaValue = t.raw_get(i)?;
                    arr.push(lua_value_to_json(&v)?);
                }
                Value::Array(arr)
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.clone().pairs::<String, LuaValue>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_value_to_json(&v)?);
                }
                Value::Object(map)
            }
        }
        _ => Value::Null,
    })
}

fn json_to_lua(lua: &Lua, value: &Value) -> mlua::Result<LuaValue> {
    Ok(match value {
        Value::Null => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Number(n) => n.as_f64().map(LuaValue::Number).unwrap_or(LuaValue::Nil),
        Value::String(s) => LuaValue::String(lua.create_string(s)?),
        Value::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, json_to_lua(lua, v)?)?;
            }
            LuaValue::Table(t)
        }
        Value::Object(obj) => {
            let t = lua.create_table()?;
            for (k, v) in obj {
                t.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

// ── LuauRenderRegistry (Phase 6a) ───────────────────────────────────

/// Owns the shared [`mlua::Lua`] state and the per-widget compiled
/// render functions. One instance per document scope (per the plan's
/// "Lua state per document" isolation decision).
pub struct LuauRenderRegistry {
    lua: Lua,
    /// component id → registry key for the compiled `render` function.
    render_fns: HashMap<String, RegistryKey>,
    /// Contributions keyed the same way, so the host can re-derive a
    /// `LuauComponent` after a hot-reload without re-parsing the
    /// table.
    contributions: HashMap<String, WidgetContribution>,
    /// Buffer that `prism.widget {...}` calls drain into. `compile`
    /// resets it on each pass so a single source file can register
    /// multiple widgets.
    pending: Arc<RefCell<Vec<PendingWidget>>>,
}

struct PendingWidget {
    contribution: WidgetContribution,
    render_key: RegistryKey,
}

impl Default for LuauRenderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LuauRenderRegistry {
    pub fn new() -> Self {
        let lua = Lua::new();
        let pending: Arc<RefCell<Vec<PendingWidget>>> = Arc::new(RefCell::new(Vec::new()));
        // Best-effort install. Caller can re-run `install_global` if a
        // host wants to re-bind after their own setup.
        let _ = install_widget_global(&lua, pending.clone());
        Self {
            lua,
            render_fns: HashMap::new(),
            contributions: HashMap::new(),
            pending,
        }
    }

    /// Reset and re-install the `prism.widget` global on top of the
    /// existing Lua state. Useful when a host wants to overwrite the
    /// daemon's stateless `prism` userdata with a richer wrapper.
    pub fn install_global(&self) -> mlua::Result<()> {
        install_widget_global(&self.lua, self.pending.clone())
    }

    /// Compile a `.luau` source file. Every `prism.widget {...}` call
    /// in the source registers a widget; this returns the resulting
    /// [`LuauComponent`]s in registration order.
    pub fn compile(&mut self, source: &str) -> mlua::Result<Vec<LuauComponent>> {
        self.pending.borrow_mut().clear();
        self.lua.load(source).exec()?;
        let drained: Vec<PendingWidget> = std::mem::take(&mut *self.pending.borrow_mut());
        let mut out = Vec::with_capacity(drained.len());
        for pw in drained {
            let id = pw.contribution.id.clone();
            // Replace any previous registration so re-running the same
            // source surface acts like hot-reload.
            if let Some(prev) = self.render_fns.remove(&id) {
                let _ = self.lua.remove_registry_value(prev);
            }
            self.render_fns.insert(id.clone(), pw.render_key);
            self.contributions
                .insert(id.clone(), pw.contribution.clone());
            out.push(LuauComponent {
                contribution: pw.contribution,
            });
        }
        Ok(out)
    }

    /// Re-compile a single widget's source. The plan calls for this to
    /// be invoked from the VFS watcher; today it's also the inner
    /// implementation behind `compile` for re-runs.
    pub fn replace(&mut self, component_id: &str, source: &str) -> mlua::Result<LuauComponent> {
        let registered = self.compile(source)?;
        registered
            .into_iter()
            .find(|c| c.contribution.id == component_id)
            .ok_or_else(|| {
                mlua::Error::external(format!(
                    "source did not register a widget with id `{component_id}`"
                ))
            })
    }

    pub fn contains(&self, component_id: &str) -> bool {
        self.render_fns.contains_key(component_id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.render_fns.keys().map(String::as_str)
    }

    /// Invoke a registered render function. Returns the parsed virtual
    /// tree. Surfaces any Luau error verbatim.
    pub fn invoke(
        &self,
        component_id: &str,
        props: &Value,
        data: &Value,
    ) -> Result<VirtualNode, RenderError> {
        let key = self.render_fns.get(component_id).ok_or_else(|| {
            RenderError::Failed(format!("no Luau render fn registered for `{component_id}`"))
        })?;
        let fun: Function = self
            .lua
            .registry_value(key)
            .map_err(|e| RenderError::Failed(format!("Luau registry: {e}")))?;
        let props_l = json_to_lua(&self.lua, props)
            .map_err(|e| RenderError::Failed(format!("props → lua: {e}")))?;
        let data_l = json_to_lua(&self.lua, data)
            .map_err(|e| RenderError::Failed(format!("data → lua: {e}")))?;
        let ctx_t = self
            .lua
            .create_table()
            .map_err(|e| RenderError::Failed(format!("ctx table: {e}")))?;
        let v: LuaValue = fun
            .call((props_l, data_l, ctx_t))
            .map_err(|e| RenderError::Failed(format!("Luau render `{component_id}`: {e}")))?;
        VirtualNode::from_lua(v, &self.lua)
            .map_err(|e| RenderError::Failed(format!("Luau render → VirtualNode: {e}")))
    }
}

// ── prism.widget global helper (Phase 6c) ───────────────────────────

fn install_widget_global(lua: &Lua, pending: Arc<RefCell<Vec<PendingWidget>>>) -> mlua::Result<()> {
    // Either grab the existing `prism` userdata and wrap it, or build a
    // fresh table. Either way the result is a table writable from the
    // Rust side.
    let globals = lua.globals();
    let existing: LuaValue = globals.get("prism").unwrap_or(LuaValue::Nil);
    let wrapper = lua.create_table()?;
    if !matches!(existing, LuaValue::Nil) {
        let mt = lua.create_table()?;
        mt.set("__index", existing)?;
        wrapper.set_metatable(Some(mt));
    }

    let widget_fn = {
        let pending = pending.clone();
        let lua_clone = lua.clone();
        lua.create_function(move |_, table: Table| {
            let contribution = parse_contribution(&table)?;
            let render: Function = table.get("render").map_err(|e| {
                mlua::Error::external(format!(
                    "prism.widget `{}`: `render` field is required ({e})",
                    contribution.id
                ))
            })?;
            let render_key = lua_clone.create_registry_value(render)?;
            pending.borrow_mut().push(PendingWidget {
                contribution,
                render_key,
            });
            Ok(())
        })?
    };
    wrapper.set("widget", widget_fn)?;
    install_field_helpers(lua, &wrapper)?;
    install_signal_helper(lua, &wrapper)?;
    install_axis_helper(lua, &wrapper)?;
    globals.set("prism", wrapper)?;
    Ok(())
}

fn install_field_helpers(lua: &Lua, wrapper: &Table) -> mlua::Result<()> {
    let field = lua.create_table()?;
    field.set(
        "text",
        lua.create_function(|lua, (key, opts): (String, Option<Table>)| {
            let label = opts
                .as_ref()
                .and_then(|t| t.get::<String>("label").ok())
                .unwrap_or_else(|| key.clone());
            spec_to_lua_table(lua, &BuilderFieldSpec::text(key, label))
        })?,
    )?;
    field.set(
        "boolean",
        lua.create_function(|lua, (key, opts): (String, Option<Table>)| {
            let label = opts
                .as_ref()
                .and_then(|t| t.get::<String>("label").ok())
                .unwrap_or_else(|| key.clone());
            spec_to_lua_table(lua, &BuilderFieldSpec::boolean(key, label))
        })?,
    )?;
    field.set(
        "select",
        lua.create_function(|lua, (key, opts): (String, Table)| {
            let label = opts.get::<String>("label").unwrap_or_else(|_| key.clone());
            let options: Vec<String> = opts.get::<Vec<String>>("options").unwrap_or_default();
            let selopts: Vec<crate::registry::SelectOption> = options
                .into_iter()
                .map(|v| crate::registry::SelectOption::new(v.clone(), v))
                .collect();
            spec_to_lua_table(lua, &BuilderFieldSpec::select(key, label, selopts))
        })?,
    )?;
    wrapper.set("field", field)?;
    Ok(())
}

fn install_signal_helper(lua: &Lua, wrapper: &Table) -> mlua::Result<()> {
    let signal = lua.create_function(|lua, (name, payload): (String, Option<Table>)| {
        let mut spec = SignalSpec::new(name, "");
        if let Some(p) = payload {
            let mut fields = Vec::new();
            let len = p.raw_len();
            for i in 1..=len {
                let row: Table = p.raw_get(i)?;
                let key: String = row.get("name").unwrap_or_default();
                let label: String = row.get("label").unwrap_or_else(|_| key.clone());
                let kind: String = row.get("kind").unwrap_or_else(|_| "text".into());
                let f = match kind.as_str() {
                    "boolean" => BuilderFieldSpec::boolean(key, label),
                    _ => BuilderFieldSpec::text(key, label),
                };
                fields.push(f);
            }
            spec = spec.with_payload(fields);
        }
        spec_to_lua_table(lua, &spec)
    })?;
    wrapper.set("signal", signal)?;
    Ok(())
}

fn install_axis_helper(lua: &Lua, wrapper: &Table) -> mlua::Result<()> {
    let axis = lua.create_function(|lua, (key, options): (String, Vec<Table>)| {
        let label = key.clone();
        let mut variant_opts = Vec::new();
        for opt in options {
            let id: String = opt.get("id").unwrap_or_default();
            let opt_label: String = opt.get("label").unwrap_or_else(|_| id.clone());
            let overrides: Value = match opt.get::<LuaValue>("overrides") {
                Ok(v) => lua_value_to_json(&v).unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            variant_opts.push(VariantOptionSpec {
                value: id,
                label: opt_label,
                overrides,
            });
        }
        let spec = VariantSpec {
            key,
            label,
            options: variant_opts,
        };
        spec_to_lua_table(lua, &spec)
    })?;
    wrapper.set("axis", axis)?;
    Ok(())
}

fn spec_to_lua_table<T: serde::Serialize>(lua: &Lua, value: &T) -> mlua::Result<Table> {
    let json =
        serde_json::to_value(value).map_err(|e| mlua::Error::external(format!("serde: {e}")))?;
    match json_to_lua(lua, &json)? {
        LuaValue::Table(t) => Ok(t),
        _ => lua.create_table(),
    }
}

fn parse_contribution(table: &Table) -> mlua::Result<WidgetContribution> {
    let id: String = table
        .get("id")
        .map_err(|_| mlua::Error::external("prism.widget: `id` is required"))?;
    let label: String = table.get("label").unwrap_or_else(|_| id.clone());
    let description: String = table.get("description").unwrap_or_default();
    let category = match table.get::<String>("category").ok().as_deref() {
        Some("display") | None => WidgetCategory::Display,
        Some("input") => WidgetCategory::Input,
        Some("navigation") => WidgetCategory::Navigation,
        Some("data-table") | Some("DataTable") => WidgetCategory::DataTable,
        Some("temporal") => WidgetCategory::Temporal,
        Some("communication") => WidgetCategory::Communication,
        Some("finance") => WidgetCategory::Finance,
        Some("layout") => WidgetCategory::Layout,
        _ => WidgetCategory::Custom,
    };

    let config_fields = parse_field_array(table, "schema").unwrap_or_default();
    let signals = parse_signal_array(table, "signals").unwrap_or_default();
    let variants = parse_variant_array(table, "variants").unwrap_or_default();

    Ok(WidgetContribution {
        id,
        label,
        description,
        category,
        config_fields,
        signals,
        variants,
        // Render is invoked dynamically; the static template is left
        // empty because LuauComponent uses the `render` function path
        // instead of `WidgetTemplate`.
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Vertical,
                gap: None,
                padding: None,
                children: Vec::new(),
            },
        },
        default_size: WidgetSize::default(),
        ..Default::default()
    })
}

fn parse_field_array(table: &Table, key: &str) -> mlua::Result<Vec<FieldSpec>> {
    let val: LuaValue = table.get(key).unwrap_or(LuaValue::Nil);
    let arr = match val {
        LuaValue::Nil => return Ok(Vec::new()),
        LuaValue::Table(t) => t,
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    let len = arr.raw_len();
    for i in 1..=len {
        let v: LuaValue = arr.raw_get(i)?;
        let json = lua_value_to_json(&v)?;
        if let Ok(spec) = serde_json::from_value::<FieldSpec>(json) {
            out.push(spec);
        }
    }
    Ok(out)
}

fn parse_signal_array(table: &Table, key: &str) -> mlua::Result<Vec<SignalSpec>> {
    let val: LuaValue = table.get(key).unwrap_or(LuaValue::Nil);
    let arr = match val {
        LuaValue::Nil => return Ok(Vec::new()),
        LuaValue::Table(t) => t,
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    let len = arr.raw_len();
    for i in 1..=len {
        let v: LuaValue = arr.raw_get(i)?;
        let json = lua_value_to_json(&v)?;
        if let Ok(spec) = serde_json::from_value::<SignalSpec>(json) {
            out.push(spec);
        }
    }
    Ok(out)
}

fn parse_variant_array(table: &Table, key: &str) -> mlua::Result<Vec<VariantSpec>> {
    let val: LuaValue = table.get(key).unwrap_or(LuaValue::Nil);
    let arr = match val {
        LuaValue::Nil => return Ok(Vec::new()),
        LuaValue::Table(t) => t,
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    let len = arr.raw_len();
    for i in 1..=len {
        let v: LuaValue = arr.raw_get(i)?;
        let json = lua_value_to_json(&v)?;
        if let Ok(spec) = serde_json::from_value::<VariantSpec>(json) {
            out.push(spec);
        }
    }
    Ok(out)
}

// ── LuauComponent (Phase 6a, render-side) ───────────────────────────

// Thread-local registry the `Block` impls reach for at render time.
// `LuauComponent` itself is `Send + Sync` data; the actual `mlua::Lua`
// must stay on the thread that built it. The shell installs the
// registry into this slot before invoking the document walker.
thread_local! {
    static ACTIVE_REGISTRY: RefCell<Option<*const RefCell<LuauRenderRegistry>>> =
        const { RefCell::new(None) };
}

/// RAII guard that installs `registry` into the thread-local slot for
/// the duration of a render pass. Panics in `with_registry` are
/// caught by the [`Drop`] impl so a misbehaving widget can't poison
/// other render passes.
pub struct ActiveRegistry<'a> {
    _marker: std::marker::PhantomData<&'a RefCell<LuauRenderRegistry>>,
}

impl<'a> ActiveRegistry<'a> {
    pub fn install(registry: &'a RefCell<LuauRenderRegistry>) -> Self {
        ACTIVE_REGISTRY.with(|slot| {
            *slot.borrow_mut() = Some(registry as *const _);
        });
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'a> Drop for ActiveRegistry<'a> {
    fn drop(&mut self) {
        ACTIVE_REGISTRY.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

fn with_active_registry<R>(f: impl FnOnce(&LuauRenderRegistry) -> R) -> Result<R, RenderError> {
    ACTIVE_REGISTRY.with(|slot| {
        let ptr = slot.borrow().ok_or_else(|| {
            RenderError::Failed("Luau render registry is not installed for this render pass".into())
        })?;
        // SAFETY: `ActiveRegistry` guards installation and removal so
        // the pointer is valid for the lifetime of the borrow.
        let cell = unsafe { &*ptr };
        let borrow = cell
            .try_borrow()
            .map_err(|_| RenderError::Failed("Luau registry already borrowed".into()))?;
        Ok(f(&borrow))
    })
}

/// One Luau-defined widget. Stored in [`ComponentRegistry`] alongside
/// Rust-defined components.
pub struct LuauComponent {
    contribution: WidgetContribution,
}

impl LuauComponent {
    pub fn new(contribution: WidgetContribution) -> Self {
        Self { contribution }
    }

    pub fn contribution(&self) -> &WidgetContribution {
        &self.contribution
    }

    fn render_virtual(&self, props: &Value, data: &Value) -> Result<VirtualNode, RenderError> {
        with_active_registry(|reg| reg.invoke(&self.contribution.id, props, data))?
    }
}

impl Block for LuauComponent {
    fn id(&self) -> &ComponentId {
        &self.contribution.id
    }

    fn schema(&self) -> Vec<BuilderFieldSpec> {
        self.contribution.config_fields.clone()
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mapped: Vec<SignalDef> = self
            .contribution
            .signals
            .iter()
            .map(|s| SignalDef {
                name: s.name.clone(),
                description: s.description.clone(),
                payload: s.payload_fields.clone(),
            })
            .collect();
        crate::signal::with_common_signals(mapped)
    }

    fn variants(&self) -> Vec<VariantAxis> {
        self.contribution
            .variants
            .iter()
            .map(|spec| VariantAxis {
                key: spec.key.clone(),
                label: spec.label.clone(),
                options: spec
                    .options
                    .iter()
                    .map(|o| crate::variant::VariantOption {
                        value: o.value.clone(),
                        label: o.label.clone(),
                        overrides: o.overrides.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let data = Value::Array(Vec::new());
        let virt = self.render_virtual(props, &data)?;
        let mut next_id: u64 = 0;
        let node = virt.into_node(&mut next_id);
        let _ = children;
        ctx.render_child(&node, out)
    }

    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let data = Value::Array(Vec::new());
        let virt = self.render_virtual(props, &data)?;
        let mut next_id: u64 = 0;
        let node = virt.into_node(&mut next_id);
        let _ = children;
        ctx.render_child(&node, out)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_source() -> &'static str {
        r#"
        prism.widget {
            id = "kanban-luau",
            label = "Kanban (Luau)",
            schema = {
                prism.field.text("title", { label = "Title" }),
            },
            render = function(props, data, ctx)
                return {
                    component = "container",
                    props = { spacing = 8 },
                    children = {
                        { component = "text", props = { body = props.title or "Untitled" } },
                    },
                }
            end,
        }
        "#
    }

    #[test]
    fn compile_registers_widget() {
        let mut reg = LuauRenderRegistry::new();
        let comps = reg.compile(sample_source()).expect("compile");
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].id(), "kanban-luau");
        assert!(reg.contains("kanban-luau"));
    }

    #[test]
    fn missing_id_errors() {
        let mut reg = LuauRenderRegistry::new();
        let res = reg.compile(
            r#"prism.widget { label = "no id", render = function() return { component = "text" } end }"#,
        );
        let err = res.err().expect("compile should fail without id");
        assert!(err.to_string().contains("`id` is required"));
    }

    #[test]
    fn invoke_returns_virtual_tree() {
        let mut reg = LuauRenderRegistry::new();
        reg.compile(sample_source()).unwrap();
        let virt = reg
            .invoke(
                "kanban-luau",
                &serde_json::json!({"title": "Hello"}),
                &Value::Null,
            )
            .unwrap();
        assert_eq!(virt.component, "container");
        assert_eq!(virt.children.len(), 1);
        assert_eq!(virt.children[0].component, "text");
        assert_eq!(virt.children[0].props["body"], "Hello");
    }

    #[test]
    fn replace_swaps_render_fn() {
        let mut reg = LuauRenderRegistry::new();
        reg.compile(sample_source()).unwrap();
        let updated = r#"
            prism.widget {
                id = "kanban-luau",
                render = function() return { component = "text", props = { body = "v2" } } end,
            }
        "#;
        reg.replace("kanban-luau", updated).unwrap();
        let virt = reg
            .invoke("kanban-luau", &Value::Null, &Value::Null)
            .unwrap();
        assert_eq!(virt.component, "text");
        assert_eq!(virt.props["body"], "v2");
    }

    #[test]
    fn virtual_node_into_node_mints_unique_ids() {
        let v = VirtualNode {
            component: "container".into(),
            props: Value::Null,
            children: vec![VirtualNode {
                component: "text".into(),
                props: Value::Null,
                children: Vec::new(),
            }],
        };
        let mut next = 0;
        let n = v.into_node(&mut next);
        assert_eq!(n.id, "luau-0");
        assert_eq!(n.children[0].id, "luau-1");
        assert_eq!(next, 2);
    }
}
