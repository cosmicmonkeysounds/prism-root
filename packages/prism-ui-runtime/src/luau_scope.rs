//! `LuauScopeFrame` — the per-document Luau state behind a
//! `<script lang="luau">` block.
//!
//! Wave A of `docs/dev/prui-luau-fusion.md` §7.1. A `.prui` file may
//! host one top-level `<script lang="luau">` block; its body runs
//! once per document load and its **top-level `local`s become
//! document-scope bindings** reachable from every `{expr}` slot.
//!
//! ## How locals are surfaced
//!
//! A top-level `local x = …` in a Lua chunk is scoped to the chunk's
//! main function — invisible after the chunk returns. To surface it
//! as a binding, the loader strips the `local` keyword at the offset
//! `prism_core::language::luau::top_level_locals` reports, so the
//! binding lands in the chunk environment (the per-document Lua
//! globals) instead. Functions are retained behind an
//! [`mlua::RegistryKey`] so the call resolver can invoke them;
//! serialisable values are also snapshotted to JSON for the cheap
//! binding-read path.
//!
//! ## Boundedness (design principle 1 of the fusion doc)
//!
//! The script runs **once** at load. Helper functions it defines are
//! pure callables invoked at attribute time by the existing
//! call-resolver seam (`try_call_owned`). Nothing here recurses,
//! schedules, or yields during the render walk — the tree-render
//! contract stays intact.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, RegistryKey, Value as LuaValue};
use serde_json::Value as JsonValue;

/// Cheap-clone handle to a document's Luau state. Cloning shares the
/// same `Lua` + harvested-binding tables (the loader forks
/// `LowerScope` constantly; an `Rc` keeps that cheap).
#[derive(Clone)]
pub struct LuauScopeFrame {
    inner: Rc<LuauScopeInner>,
}

struct LuauScopeInner {
    lua: Lua,
    /// Harvested top-level names that resolved to a callable. The
    /// `RegistryKey` keeps the function alive in the per-document Lua
    /// state for the call resolver.
    functions: HashMap<String, RegistryKey>,
    /// JSON snapshot of every harvested name that serialised cleanly
    /// (tables, numbers, strings, booleans). Functions are absent
    /// here — they live only in `functions`.
    snapshot: HashMap<String, JsonValue>,
    /// **Wave B** — anonymous closure literals (`|x| …` / `\fn(x) …
    /// end`) compiled lazily and memoised by desugared-source hash.
    /// `RefCell` because the call resolver holds `&LuauScopeFrame`
    /// (the loader forks `LowerScope` immutably) yet must populate
    /// the cache on first encounter. Single-threaded by construction
    /// — same discipline as the rest of the per-document Lua state.
    closures: RefCell<HashMap<u64, RegistryKey>>,
    /// **Wave C (`prui-luau-fusion.md` §7.7)** — `prism.macro(name,
    /// fn)` registrations harvested from the script body. The tag
    /// resolver checks this table before the host's registered-tag
    /// resolver; a hit calls `fn(attrs, children)` and splices the
    /// returned `prui[[…]]` source in place. Populated once at load
    /// (drained from the script-time collector) — immutable
    /// afterward, so a plain `HashMap` suffices.
    macros: HashMap<String, RegistryKey>,
    /// **Wave E (`prui-luau-fusion.md` §7.8)** — `prism.dialect{
    /// name, parse }` registrations. `<language name="x">…</language>`
    /// (and the `~x{…}` sigil) dispatch the raw body through
    /// `parse(source)`, which returns `prui[[…]]` source the host
    /// re-parses + lowers. Same load-once / immutable discipline as
    /// `macros`.
    dialects: HashMap<String, RegistryKey>,
    /// **Wave G (`prui-luau-fusion.md` §7.11)** — `prism.probes:on(
    /// name, fn)` subscriptions. The `probe:<name>=` attribute lowers
    /// to `data-probe-<name>`; a host event router fires matching
    /// probes through [`Self::fire_probe`]. Load-once / immutable.
    probes: HashMap<String, RegistryKey>,
}

impl std::fmt::Debug for LuauScopeFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuauScopeFrame")
            .field(
                "functions",
                &self.inner.functions.keys().collect::<Vec<_>>(),
            )
            .field("bindings", &self.inner.snapshot.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl LuauScopeFrame {
    /// Build a frame from one or more `<script lang="luau">` bodies
    /// (multiple inline blocks concatenate in source order). `tokens`
    /// is the document's design-token JSON — seeded as a `tokens`
    /// global so a script can read `tokens.colors.danger` exactly as
    /// an expression slot would.
    ///
    /// Returns `Err` with a human-readable message on a script
    /// error; the loader surfaces it the same way a PRUI parse error
    /// surfaces (inline diagnostic), and the document still renders
    /// with no script bindings.
    pub fn from_scripts(sources: &[&str], tokens: Option<&JsonValue>) -> Result<Self, String> {
        Self::from_scripts_with_scope(sources, tokens, None)
    }

    /// **Wave I (§6.2)** — like [`Self::from_scripts`] but also seeds
    /// `prism.scope` from the document/host bindings JSON (a
    /// `LowerScope::bindings_json()` snapshot), so a `<script>` can
    /// read host-provided props: `local task = prism.scope.task`.
    /// The binding values flow through unchanged; the *type* contract
    /// (`---@type Task` ↔ host `BlockSpec` schema) is enforced by the
    /// external `luau-analyze` pass, not here.
    pub fn from_scripts_with_scope(
        sources: &[&str],
        tokens: Option<&JsonValue>,
        scope_bindings: Option<&JsonValue>,
    ) -> Result<Self, String> {
        let combined = sources.join("\n");
        let transformed = strip_top_level_locals(&combined);
        let harvested: Vec<String> = prism_core::language::luau::top_level_locals(&combined)
            .into_iter()
            .flat_map(|d| d.names)
            .collect();

        let lua = Lua::new();
        // **Wave C** — `prism.macro(name, fn)` collector. The Lua
        // closure can't reach the Rust-side `macros` map (built only
        // after exec), so registrations land in this shared cell
        // during script execution and are drained afterward. Same
        // proven pattern as `prism-core`'s `LuauCallbackStore`.
        let macro_collector: Rc<RefCell<Vec<(String, RegistryKey)>>> =
            Rc::new(RefCell::new(Vec::new()));
        // **Wave E** — same collector pattern for `prism.dialect`.
        let dialect_collector: Rc<RefCell<Vec<(String, RegistryKey)>>> =
            Rc::new(RefCell::new(Vec::new()));
        // **Wave G** — `prism.probes:on(name, fn)` collector.
        let probe_collector: Rc<RefCell<Vec<(String, RegistryKey)>>> =
            Rc::new(RefCell::new(Vec::new()));
        // Install the `prism.*` helpers + `tokens` into the *base*
        // environment first, then enable Luau's sandbox. Post-sandbox
        // the standard library + these globals are frozen read-only
        // (a script can't monkey-patch `string`, `prism`, or
        // `tokens`); the script's own top-level assignments still
        // land in a writable sandbox layer the loader harvests. This
        // is the Wave A capability baseline — design principle 4 of
        // `prui-luau-fusion.md` — ahead of the richer
        // `ShellHandles::install` host-handle matrix in a later wave.
        install_prism_helpers(&lua, &macro_collector, &dialect_collector, &probe_collector)
            .map_err(|e| format!("install prism helpers: {e}"))?;
        if let Some(tokens) = tokens {
            let tokens_lua = lua
                .to_value(tokens)
                .map_err(|e| format!("seed tokens: {e}"))?;
            lua.globals()
                .set("tokens", tokens_lua)
                .map_err(|e| format!("set tokens: {e}"))?;
        }
        // **Wave I** — `prism.scope.<name>` host-binding bridge.
        // Seeded onto the `prism` table before the sandbox freezes
        // it, so the script reads host props read-only.
        if let Some(bindings) = scope_bindings {
            let scope_lua = lua
                .to_value(bindings)
                .map_err(|e| format!("seed prism.scope: {e}"))?;
            let prism: mlua::Table = lua
                .globals()
                .get("prism")
                .map_err(|e| format!("get prism table: {e}"))?;
            prism
                .set("scope", scope_lua)
                .map_err(|e| format!("set prism.scope: {e}"))?;
        }
        lua.sandbox(true)
            .map_err(|e| format!("enable luau sandbox: {e}"))?;

        lua.load(&transformed)
            .set_name("script")
            .exec()
            .map_err(|e| format!("script: {e}"))?;

        let globals = lua.globals();
        let mut functions = HashMap::new();
        let mut snapshot = HashMap::new();
        for name in harvested {
            let value: LuaValue = match globals.get(name.as_str()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match value {
                LuaValue::Function(_) => {
                    if let Ok(key) = lua.create_registry_value(value) {
                        functions.insert(name, key);
                    }
                }
                other => {
                    if let Ok(json) = lua.from_value::<JsonValue>(other) {
                        snapshot.insert(name, json);
                    }
                }
            }
        }

        let macros: HashMap<String, RegistryKey> = macro_collector.borrow_mut().drain(..).collect();
        let dialects: HashMap<String, RegistryKey> =
            dialect_collector.borrow_mut().drain(..).collect();
        let probes: HashMap<String, RegistryKey> =
            probe_collector.borrow_mut().drain(..).collect();

        Ok(Self {
            inner: Rc::new(LuauScopeInner {
                lua,
                functions,
                snapshot,
                closures: RefCell::new(HashMap::new()),
                macros,
                dialects,
                probes,
            }),
        })
    }

    /// Read a dotted path **live** from the per-document Lua globals
    /// (as opposed to [`Self::lookup`], which serves the load-time
    /// JSON snapshot). Used where a value may have mutated after
    /// load — a probe / signal handler writing back into a
    /// `prism.state` table. `None` when the path is absent or not
    /// JSON-serialisable.
    pub fn read_global(&self, path: &str) -> Option<JsonValue> {
        let lua = &self.inner.lua;
        let mut parts = path.split('.');
        let head = parts.next()?.trim();
        let mut cur: LuaValue = lua.globals().get(head).ok()?;
        for seg in parts {
            let tbl = match cur {
                LuaValue::Table(t) => t,
                _ => return None,
            };
            cur = tbl.get(seg.trim()).ok()?;
        }
        lua.from_value(cur).ok()
    }

    /// **Wave G** — is `name` a subscribed probe?
    pub fn has_probe(&self, name: &str) -> bool {
        self.inner.probes.contains_key(name)
    }

    /// **Wave G** — fire the probe `name` with a JSON payload,
    /// invoking its `prism.probes:on` handler. `None` when no
    /// handler subscribed; `Err` when the handler body failed. The
    /// host event router calls this when an interaction hits an
    /// element carrying `data-probe-<name>`.
    pub fn fire_probe(&self, name: &str, payload: &JsonValue) -> Option<Result<(), String>> {
        let key = self.inner.probes.get(name)?;
        let lua = &self.inner.lua;
        let run = || -> Result<(), String> {
            let func: mlua::Function = lua
                .registry_value(key)
                .map_err(|e| format!("resolve probe: {e}"))?;
            let arg = lua
                .to_value(payload)
                .map_err(|e| format!("serialize probe payload: {e}"))?;
            func.call::<()>(arg).map_err(|e| format!("call probe: {e}"))
        };
        Some(run())
    }

    /// **Wave E** — is `name` a script-registered sub-dialect?
    pub fn has_dialect(&self, name: &str) -> bool {
        self.inner.dialects.contains_key(name)
    }

    /// **Wave E** — run a `<language name="…">` body (or `~name{…}`
    /// sigil) through its dialect's `parse(source)`. Returns the
    /// `prui[[…]]` source string for the host to re-parse + lower.
    /// `None` when `name` isn't a dialect; `Err` on a parse-fn
    /// failure.
    pub fn expand_dialect(&self, name: &str, source: &str) -> Option<Result<String, String>> {
        let key = self.inner.dialects.get(name)?;
        Some(self.expand_dialect_inner(key, source))
    }

    fn expand_dialect_inner(&self, key: &RegistryKey, source: &str) -> Result<String, String> {
        let lua = &self.inner.lua;
        let func: mlua::Function = lua
            .registry_value(key)
            .map_err(|e| format!("resolve dialect: {e}"))?;
        func.call::<String>(source.to_string())
            .map_err(|e| format!("call dialect: {e}"))
    }

    /// **Wave C** — is `tag` a script-registered macro?
    pub fn has_macro(&self, tag: &str) -> bool {
        self.inner.macros.contains_key(tag)
    }

    /// **Wave C** — expand a macro tag. Calls the registered
    /// `fn(attrs, children)` with the caller's resolved attribute
    /// object and already-lowered child nodes (as JSON), and returns
    /// the `prui[[…]]` source string the macro produced for the host
    /// to re-parse + lower. `None` when `tag` isn't a macro; `Err`
    /// when the macro body itself failed.
    ///
    /// Hygiene: the macro runs in the document's sandboxed Lua with
    /// **only** `attrs` + `children` passed in. Its free variables
    /// resolve against the declaring script's scope (same Lua
    /// state), never the caller's PRUI bindings — Racket-style, not
    /// C-preprocessor (§7.7).
    pub fn expand_macro(
        &self,
        tag: &str,
        attrs: &JsonValue,
        children: &JsonValue,
    ) -> Option<Result<String, String>> {
        let key = self.inner.macros.get(tag)?;
        Some(self.expand_macro_inner(key, attrs, children))
    }

    fn expand_macro_inner(
        &self,
        key: &RegistryKey,
        attrs: &JsonValue,
        children: &JsonValue,
    ) -> Result<String, String> {
        let lua = &self.inner.lua;
        let func: mlua::Function = lua
            .registry_value(key)
            .map_err(|e| format!("resolve macro: {e}"))?;
        let attrs_lua = lua
            .to_value(attrs)
            .map_err(|e| format!("serialize macro attrs: {e}"))?;
        let children_lua = lua
            .to_value(children)
            .map_err(|e| format!("serialize macro children: {e}"))?;
        // `prui[[…]]` is identity over the long-bracket string, so a
        // well-formed macro returns a string. mlua coerces / errors
        // if the body returned anything else.
        func.call::<String>((attrs_lua, children_lua))
            .map_err(|e| format!("call macro: {e}"))
    }

    /// **Wave B** — desugar a closure literal and call it with JSON
    /// args, returning its result as JSON. `src` is the raw slot text
    /// (`|t| t.priority == 'high'` or `\fn(t) return -t.n end`);
    /// returns `None` when `src` isn't a closure literal so the
    /// caller falls through. The compiled function is memoised by
    /// desugared-source hash inside the per-document Lua state, so a
    /// closure used once per array element compiles exactly once.
    ///
    /// Boundedness: the closure runs in the same sandboxed Lua as the
    /// script block (stdlib frozen, no `os`/`io`), is called with the
    /// element value, returns a value, and the walker continues —
    /// design principle 1 holds.
    pub fn call_closure(&self, src: &str, args: &[JsonValue]) -> Option<Result<JsonValue, String>> {
        let lua_src = desugar_closure(src)?;
        Some(self.call_closure_inner(&lua_src, args))
    }

    fn call_closure_inner(&self, lua_src: &str, args: &[JsonValue]) -> Result<JsonValue, String> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        lua_src.hash(&mut hasher);
        let key = hasher.finish();
        let lua = &self.inner.lua;

        let need_compile = !self.inner.closures.borrow().contains_key(&key);
        if need_compile {
            let func: mlua::Function = lua
                .load(lua_src)
                .set_name("closure")
                .eval()
                .map_err(|e| format!("compile closure: {e}"))?;
            let rk = lua
                .create_registry_value(func)
                .map_err(|e| format!("retain closure: {e}"))?;
            self.inner.closures.borrow_mut().insert(key, rk);
        }

        let func: mlua::Function = {
            let cache = self.inner.closures.borrow();
            let rk = cache.get(&key).expect("just inserted");
            lua.registry_value(rk)
                .map_err(|e| format!("resolve closure: {e}"))?
        };
        let mut lua_args = mlua::MultiValue::new();
        for arg in args.iter().rev() {
            let v = lua
                .to_value(arg)
                .map_err(|e| format!("serialize closure arg: {e}"))?;
            lua_args.push_front(v);
        }
        let ret: LuaValue = func
            .call(lua_args)
            .map_err(|e| format!("call closure: {e}"))?;
        lua.from_value(ret)
            .map_err(|e| format!("decode closure result: {e}"))
    }

    /// Resolve a dotted path (`state.expanded`, `task.title`) against
    /// the harvested value snapshot. Returns `None` when the head
    /// isn't a harvested binding or a segment doesn't exist — the
    /// caller falls through to the other resolution layers.
    pub fn lookup(&self, path: &str) -> Option<JsonValue> {
        let path = path.trim();
        let mut parts = path.split('.');
        let head = parts.next()?.trim();
        let mut cursor = self.inner.snapshot.get(head)?;
        for seg in parts {
            let key = seg.trim();
            cursor = match cursor {
                JsonValue::Object(map) => map.get(key)?,
                JsonValue::Array(arr) => arr.get(key.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(cursor.clone())
    }

    /// True when `name` is a harvested top-level callable — the call
    /// resolver checks this before parsing an argument list.
    pub fn has_function(&self, name: &str) -> bool {
        self.inner.functions.contains_key(name)
    }

    /// Invoke a harvested function with JSON args, returning its
    /// result as JSON. `None` when `name` isn't a callable; `Err`
    /// when the call itself failed (surfaces as an inline diagnostic
    /// upstream, never a panic).
    pub fn call(&self, name: &str, args: &[JsonValue]) -> Option<Result<JsonValue, String>> {
        let key = self.inner.functions.get(name)?;
        Some(self.call_inner(key, args))
    }

    fn call_inner(&self, key: &RegistryKey, args: &[JsonValue]) -> Result<JsonValue, String> {
        let lua = &self.inner.lua;
        let func: mlua::Function = lua
            .registry_value(key)
            .map_err(|e| format!("resolve fn: {e}"))?;
        let mut lua_args = mlua::MultiValue::new();
        // Push in reverse: `MultiValue` is a stack, front-popped by
        // the callee, so the first arg must be pushed last.
        for arg in args.iter().rev() {
            let v = lua
                .to_value(arg)
                .map_err(|e| format!("serialize arg: {e}"))?;
            lua_args.push_front(v);
        }
        let ret: LuaValue = func.call(lua_args).map_err(|e| format!("call: {e}"))?;
        lua.from_value(ret)
            .map_err(|e| format!("decode result: {e}"))
    }
}

/// Install the Wave A baseline `prism.*` helper table.
///
/// - `prism.state(t)` → the table verbatim. Reactivity (signal
///   wiring, write-triggered re-lower) lands in a later wave; for
///   Wave A a state table is a plain readable binding.
/// - `prism.derive(fn)` → `fn()` evaluated once at load. Memoised
///   re-evaluation also lands later.
/// - `prism.on_mount` / `on_update` / `on_cleanup` → registered but
///   not yet fired (block lifecycle wiring is a later wave). Kept so
///   a script that calls them loads cleanly today.
/// - **Wave C** `prui(src)` global → identity over the source
///   string (the host re-parses it); `prism.macro(name, fn)` →
///   records `(name, fn)` into the macro collector for the tag
///   resolver to dispatch through.
///
/// **Wave E.3** — `prui_ast.*` constructor prelude. Frozen by the
/// sandbox (loaded before `sandbox(true)`); pure string building.
const PRUI_AST_PRELUDE: &str = r#"
prui_ast = {}
local function esc(v)
  return tostring(v):gsub('"', '&quot;')
end
local function attrs_str(a)
  if type(a) ~= "table" then return "" end
  local s = ""
  for k, v in pairs(a) do
    if k ~= "children" and k ~= "text" then
      local key = tostring(k):gsub("_", "-")
      s = s .. " " .. key .. '="' .. esc(v) .. '"'
    end
  end
  return s
end
local function children_str(c)
  if c == nil then return "" end
  if type(c) == "string" then return c end
  local s = ""
  for _, ch in ipairs(c) do s = s .. tostring(ch) end
  return s
end
local function container_like(tag)
  return function(spec)
    spec = spec or {}
    local body = children_str(spec.children)
    if spec.text ~= nil then body = esc(spec.text) .. body end
    return "<" .. tag .. attrs_str(spec) .. ">" .. body .. "</" .. tag .. ">"
  end
end
local function text_like(tag)
  return function(content, a)
    return "<" .. tag .. attrs_str(a) .. ">" .. esc(content) .. "</" .. tag .. ">"
  end
end
local function void_like(tag)
  return function(a) return "<" .. tag .. attrs_str(a) .. "/>" end
end
prui_ast.container = container_like("container")
prui_ast.fragment  = function(c) return "<fragment>" .. children_str(c) .. "</fragment>" end
prui_ast.text      = text_like("text")
prui_ast.heading   = text_like("heading")
prui_ast.spacer    = void_like("spacer")
prui_ast.image     = void_like("image")
"#;

fn install_prism_helpers(
    lua: &Lua,
    macro_collector: &Rc<RefCell<Vec<(String, RegistryKey)>>>,
    dialect_collector: &Rc<RefCell<Vec<(String, RegistryKey)>>>,
    probe_collector: &Rc<RefCell<Vec<(String, RegistryKey)>>>,
) -> mlua::Result<()> {
    let prism = lua.create_table()?;

    let state = lua.create_function(|_, t: LuaValue| Ok(t))?;
    prism.set("state", state)?;

    let derive = lua.create_function(|_, f: mlua::Function| f.call::<LuaValue>(()))?;
    prism.set("derive", derive)?;

    for hook in ["on_mount", "on_update", "on_cleanup"] {
        let noop = lua.create_function(|_, _f: mlua::Function| Ok(()))?;
        prism.set(hook, noop)?;
    }

    // **Wave C** — `prui[[ … ]]` quasi-quote. Lua long-bracket
    // strings need no escaping, so the body arrives here verbatim;
    // `prui` is identity — the host re-parses the string through the
    // same `prism_core::language::prism_ui::parse` the `.prui` files
    // use, with `attrs` / `children` bound in the lowering scope.
    let prui = lua.create_function(|_, src: String| Ok(src))?;
    lua.globals().set("prui", prui)?;

    // **Wave E.3 / §7.8** — `prui_ast.*` programmatic constructors.
    // A dialect/macro that builds a tree node-by-node (rather than
    // string-templating `prui[[…]]`) uses these; each returns the
    // same PRUI source string the host re-parses, so the two styles
    // compose. Authored as a frozen Lua prelude (loaded pre-sandbox)
    // — pure string building, no host calls. `_`→`-` key rewrite so
    // `font_size = 12` emits `font-size="12"`.
    lua.load(PRUI_AST_PRELUDE).set_name("prui_ast").exec()?;

    // **Wave C** — `prism.macro(name, fn)`. Retains `fn` in the
    // registry and records the pair for post-exec drain into the
    // frame's macro table.
    let collector = Rc::clone(macro_collector);
    let macro_fn = lua.create_function(move |lua, (name, f): (String, mlua::Function)| {
        let key = lua.create_registry_value(f)?;
        collector.borrow_mut().push((name, key));
        Ok(())
    })?;
    prism.set("macro", macro_fn)?;

    // **Wave E** — `prism.dialect { name = …, parse = fn }`. The
    // table form matches the doc; we pull `name` + `parse` and
    // record them like a macro.
    let dcollector = Rc::clone(dialect_collector);
    let dialect_fn = lua.create_function(move |lua, spec: mlua::Table| {
        let name: String = spec.get("name")?;
        let parse: mlua::Function = spec.get("parse")?;
        let key = lua.create_registry_value(parse)?;
        dcollector.borrow_mut().push((name, key));
        Ok(())
    })?;
    prism.set("dialect", dialect_fn)?;

    // **Wave G** — `prism.probes:on(name, fn)`. Method-call form
    // passes the `probes` table as the implicit first arg, so `on`
    // takes `(_self, name, fn)`.
    let probes_tbl = lua.create_table()?;
    let pcollector = Rc::clone(probe_collector);
    let on_fn = lua.create_function(
        move |lua, (_this, name, f): (mlua::Value, String, mlua::Function)| {
            let key = lua.create_registry_value(f)?;
            pcollector.borrow_mut().push((name, key));
            Ok(())
        },
    )?;
    probes_tbl.set("on", on_fn)?;
    prism.set("probes", probes_tbl)?;

    lua.globals().set("prism", prism)?;
    Ok(())
}

/// Rewrite a chunk so every **top-level** `local` declaration loses
/// its `local` keyword, turning a chunk-scoped binding into a
/// chunk-environment (globals) assignment the loader can harvest.
/// Offsets come from `prism_core`'s full-moon walk; we splice
/// right-to-left so earlier offsets stay valid.
///
/// `local function f` → `function f` (global function declaration).
/// `local x = 1` → `x = 1`. Nested locals are untouched — only
/// chunk-level statements are reported.
fn strip_top_level_locals(source: &str) -> String {
    let mut decls = prism_core::language::luau::top_level_locals(source);
    decls.sort_by(|a, b| b.local_keyword_offset.cmp(&a.local_keyword_offset));
    let mut out = source.to_string();
    for decl in decls {
        let start = decl.local_keyword_offset;
        // Remove `local` + the single following whitespace run so
        // `local  x` collapses cleanly to `x`.
        let after_kw = start + "local".len();
        let mut end = after_kw;
        for (i, ch) in source[after_kw..].char_indices() {
            if ch.is_whitespace() {
                end = after_kw + i + ch.len_utf8();
            } else {
                break;
            }
        }
        if out.is_char_boundary(start) && out.is_char_boundary(end) {
            out.replace_range(start..end, "");
        }
    }
    out
}

/// **Wave B** — desugar an expression-slot closure literal into a
/// Lua function-expression source string. Two surface forms
/// (`prui-luau-fusion.md` §7.2):
///
/// - `|args| expr` → `function(args) return (expr) end` — the terse
///   single-expression form (Rust/Ruby-flavoured).
/// - `\fn(args) … end` → `function(args) … end` — the multi-line
///   form; `\fn` is literally `function` with the keyword elided so
///   the body already carries its own `return`/`end`.
///
/// Returns `None` when `src` isn't a closure literal, so the call
/// resolver can fall through to the existing field-name builtin
/// form (`filter(arr, "field", value)`).
fn desugar_closure(src: &str) -> Option<String> {
    let s = src.trim();
    if let Some(rest) = s.strip_prefix("\\fn") {
        // `\fn(args) body end` — rest begins at `(`. Reject a bare
        // `\fnfoo` (identifier) by requiring `(` or whitespace next.
        let next = rest.trim_start();
        if next.starts_with('(') {
            return Some(format!("function{rest}"));
        }
        return None;
    }
    if let Some(rest) = s.strip_prefix('|') {
        // `|args| body` — split on the closing bar of the arg list.
        let close = rest.find('|')?;
        let args = rest[..close].trim();
        let body = rest[close + 1..].trim();
        if body.is_empty() {
            return None;
        }
        return Some(format!("function({args}) return ({body}) end"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desugar_pipe_bar_closure() {
        assert_eq!(
            desugar_closure("|t| t.priority == 'high'").as_deref(),
            Some("function(t) return (t.priority == 'high') end")
        );
    }

    #[test]
    fn desugar_fn_closure() {
        assert_eq!(
            desugar_closure("\\fn(t) return -t.priority end").as_deref(),
            Some("function(t) return -t.priority end")
        );
    }

    #[test]
    fn desugar_rejects_non_closure() {
        assert_eq!(desugar_closure("t.priority"), None);
        assert_eq!(desugar_closure("filter(xs, 'a', 1)"), None);
    }

    #[test]
    fn closure_call_and_cache() {
        let frame = LuauScopeFrame::from_scripts(&[], None).expect("empty frame");
        // First call compiles + caches.
        let r = frame
            .call_closure("|x| x * 2", &[JsonValue::from(21)])
            .expect("closure")
            .expect("ok");
        assert_eq!(r, JsonValue::from(42));
        // Second identical call hits the cache (one retained entry).
        let r2 = frame
            .call_closure("|x| x * 2", &[JsonValue::from(5)])
            .expect("closure")
            .expect("ok");
        assert_eq!(r2, JsonValue::from(10));
        assert_eq!(frame.inner.closures.borrow().len(), 1);
        // `\fn` multi-statement form.
        let r3 = frame
            .call_closure(
                "\\fn(t) return t.priority end",
                &[serde_json::json!({ "priority": "high" })],
            )
            .expect("closure")
            .expect("ok");
        assert_eq!(r3, JsonValue::from("high"));
    }

    #[test]
    fn strip_keeps_nested_local() {
        let src = "local x = 1\nlocal function f()\n  local y = 2\n  return y\nend";
        let out = strip_top_level_locals(src);
        assert!(out.starts_with("x = 1"));
        assert!(out.contains("function f()"));
        // The nested `local y` survives — only chunk-level strips.
        assert!(out.contains("local y = 2"));
    }

    #[test]
    fn harvests_helper_and_state() {
        let src = r##"
            local function priority_color(p)
              if p == "high" then return "#ff0000" end
              return "#888888"
            end
            local state = prism.state { expanded = false, count = 3 }
        "##;
        let frame = LuauScopeFrame::from_scripts(&[src], None).expect("frame");
        assert!(frame.has_function("priority_color"));
        assert_eq!(frame.lookup("state.expanded"), Some(JsonValue::Bool(false)));
        assert_eq!(frame.lookup("state.count"), Some(JsonValue::from(3)));
        let got = frame
            .call("priority_color", &[JsonValue::from("high")])
            .expect("callable")
            .expect("ok");
        assert_eq!(got, JsonValue::from("#ff0000"));
    }

    #[test]
    fn derive_runs_once_at_load() {
        let src = r#"
            local base = 10
            local doubled = prism.derive(function() return base * 2 end)
        "#;
        let frame = LuauScopeFrame::from_scripts(&[src], None).expect("frame");
        assert_eq!(frame.lookup("doubled"), Some(JsonValue::from(20)));
    }

    #[test]
    fn tokens_seeded_into_script_scope() {
        let tokens = serde_json::json!({ "colors": { "danger": "#e00" } });
        let src = r#"local danger = tokens.colors.danger"#;
        let frame = LuauScopeFrame::from_scripts(&[src], Some(&tokens)).expect("frame");
        assert_eq!(frame.lookup("danger"), Some(JsonValue::from("#e00")));
    }

    #[test]
    fn script_error_surfaces_as_err() {
        let err = LuauScopeFrame::from_scripts(&["local x = nil + {}"], None);
        assert!(err.is_err());
    }
}
