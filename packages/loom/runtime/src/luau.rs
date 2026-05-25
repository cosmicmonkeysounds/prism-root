//! Luau bridge for the Loom v3 directive registry (spec §14).
//!
//! Wraps an [`mlua::Lua`] state, exposes a `loom` global table with
//! read/write views into the runtime [`World`] + [`Ledger`], and lets
//! the playhead dispatch a parsed [`crate::directives::DirectiveCall`]
//! through a Luau function call following the spec §14.1 argument
//! convention (`<kind: a, b, key: value>` →
//! `kind(a, b, { key = value })`).
//!
//! ## Argument convention (§14.1)
//!
//! Positional arguments are passed first, in source order. If any
//! named arguments are present, a single trailing table containing the
//! named pairs is appended. A directive with **only** named args
//! receives one argument (the table). A bare `<kind>` receives no args.
//!
//! ## Backend
//!
//! Uses the workspace-pinned `mlua` with the `luau` feature. The
//! syntax we accept inside `directive name(args) … end` (function
//! declaration, locals, table literals, `if`/`return`) is a strict
//! subset shared with Lua 5.4 — if a future host platform fails to
//! link the Luau toolchain, swapping the feature to `lua54` will keep
//! the bridge working without changing any of the Rust call sites.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use indexmap::IndexMap;
use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue, Variadic};

use crate::directives::{
    Assign, AssignOp, CallContext, DirectiveCall, DirectiveError, HandlerOutcome,
};
use crate::expr::{self, ExprError, Value, World};
use crate::ledger::{Event, Ledger};

/// Side-effect domains that core builtins call into. Extension authors
/// can override these by registering a Luau function with the same
/// name through the `directive` keyword (last write wins, mirroring
/// the spec).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainEvent {
    /// Audio: `sfx`, `cue`, `compose`, `broadcast`.
    Audio,
    /// Lighting / staging: `flash`, `goto`.
    Lighting,
    /// Simulacra: `spawn`, `cancel`, `goal`, `heal`, `enroll`.
    Sim,
    /// Generic envelope — surfaced as `Event::Directive { … }`.
    Envelope,
}

/// Mutable slice handed to Luau callbacks during one dispatch. Holds
/// raw pointers because mlua callbacks must be `'static`; the pointers
/// are only dereferenced while [`LuauRegistry::dispatch`] sits on the
/// call stack, which holds the exclusive borrows alive.
struct DispatchSlot {
    world: *mut World,
    ledger: *mut Ledger,
    /// Last-seen outcome reported by a directive. Defaults to
    /// [`HandlerOutcome::Handled`] — handlers that fully push their
    /// own events should call `loom.suppress()`.
    suppress: bool,
}

impl DispatchSlot {
    /// SAFETY: caller must hold the original mutable borrows for the
    /// lifetime of the returned references.
    #[allow(clippy::mut_from_ref)]
    unsafe fn world(&self) -> &mut World {
        &mut *self.world
    }
    #[allow(clippy::mut_from_ref)]
    unsafe fn ledger(&self) -> &mut Ledger {
        &mut *self.ledger
    }
}

type Slot = Rc<RefCell<Option<DispatchSlot>>>;

/// The Luau-backed directive registry. Owns a single `mlua::Lua` and
/// the slot used to pass borrows in/out of callbacks.
pub struct LuauRegistry {
    lua: Lua,
    slot: Slot,
}

impl LuauRegistry {
    /// Build a fresh registry with the `loom` global wired up.
    pub fn new() -> Result<Self, DirectiveError> {
        let lua = Lua::new();
        let slot: Slot = Rc::new(RefCell::new(None));
        install_loom_global(&lua, &slot)?;
        let reg = Self { lua, slot };
        Ok(reg)
    }

    /// Build a registry with the core builtins (§14) preinstalled.
    pub fn with_core_builtins() -> Result<Self, DirectiveError> {
        let mut reg = Self::new()?;
        register_core_builtins(&mut reg)?;
        Ok(reg)
    }

    /// Register a Rust closure under `name`. Used by `register_core_builtins`.
    pub fn register_rust<F>(&mut self, name: &str, f: F) -> Result<(), DirectiveError>
    where
        F: Fn(&Lua, Variadic<LuaValue>) -> mlua::Result<LuaValue> + 'static,
    {
        let func = self.lua.create_function(f).map_err(map_lua_err)?;
        let registry: Table = self.lua.globals().get("_loom").map_err(map_lua_err)?;
        let handlers: Table = registry.get("handlers").map_err(map_lua_err)?;
        handlers.set(name, func).map_err(map_lua_err)?;
        Ok(())
    }

    /// Returns true if a handler with `name` is registered.
    pub fn contains(&self, name: &str) -> bool {
        let Ok(registry) = self.lua.globals().get::<Table>("_loom") else {
            return false;
        };
        let Ok(handlers) = registry.get::<Table>("handlers") else {
            return false;
        };
        handlers.contains_key(name).unwrap_or(false)
    }

    /// Execute a `.luau` extension file. The script may call
    /// `directive name(args) … end` (the `directive` global) or write
    /// directly to `_loom.handlers[name] = function …`.
    pub fn load_extension(&self, path: &Path) -> Result<(), DirectiveError> {
        let src = std::fs::read_to_string(path).map_err(|e| DirectiveError::BadArgs {
            kind: "<extension>".into(),
            message: format!("read {}: {e}", path.display()),
        })?;
        self.lua
            .load(&src)
            .set_name(path.display().to_string())
            .exec()
            .map_err(map_lua_err)
    }

    /// Execute an extension from an in-memory source string. Useful
    /// for tests.
    pub fn load_extension_str(&self, name: &str, source: &str) -> Result<(), DirectiveError> {
        self.lua
            .load(source)
            .set_name(name)
            .exec()
            .map_err(map_lua_err)
    }

    /// Dispatch one parsed [`DirectiveCall`].
    pub fn dispatch(
        &self,
        call: &DirectiveCall,
        world: &mut World,
        ledger: &mut Ledger,
    ) -> Result<crate::directives::DispatchResult, DirectiveError> {
        if !self.contains(&call.kind) {
            return Err(DirectiveError::UnknownKind(call.kind.clone()));
        }

        let positional = eval_positional(&call.positional, world)?;
        let named = eval_named(&call.named, world)?;

        // Stash mutable refs in the slot for the duration of the call.
        {
            let mut g = self.slot.borrow_mut();
            *g = Some(DispatchSlot {
                world: world as *mut World,
                ledger: ledger as *mut Ledger,
                suppress: false,
            });
        }

        let result = self.invoke(&call.kind, &positional, &named, call.assign.as_ref());

        let suppress = self
            .slot
            .borrow()
            .as_ref()
            .map(|s| s.suppress)
            .unwrap_or(false);

        // Clear the slot before bubbling any error.
        *self.slot.borrow_mut() = None;

        result?;
        let outcome = if suppress {
            HandlerOutcome::Suppressed
        } else {
            HandlerOutcome::Handled
        };
        Ok((outcome, positional, named))
    }

    /// Compatibility shim — present so the playhead can build a
    /// [`CallContext`] for code paths that still want the trait-object
    /// shape. Currently unused by the playhead; kept for legacy tests.
    #[doc(hidden)]
    pub fn dispatch_with_ctx(
        &self,
        ctx: &mut CallContext<'_>,
    ) -> Result<HandlerOutcome, DirectiveError> {
        let call = DirectiveCall {
            kind: ctx.kind.into(),
            positional: Vec::new(),
            named: IndexMap::new(),
            assign: ctx.assign.cloned(),
        };
        // Args are already evaluated in ctx; bypass evaluation.
        {
            let mut g = self.slot.borrow_mut();
            *g = Some(DispatchSlot {
                world: ctx.world as *mut World,
                ledger: ctx.ledger as *mut Ledger,
                suppress: false,
            });
        }
        let result = self.invoke(
            ctx.kind,
            ctx.positional,
            ctx.named,
            ctx.assign,
        );
        let suppress = self
            .slot
            .borrow()
            .as_ref()
            .map(|s| s.suppress)
            .unwrap_or(false);
        *self.slot.borrow_mut() = None;
        result?;
        Ok(if suppress {
            HandlerOutcome::Suppressed
        } else {
            HandlerOutcome::Handled
        })
        .map(|o| {
            let _ = &call;
            o
        })
    }

    fn invoke(
        &self,
        name: &str,
        positional: &[Value],
        named: &IndexMap<String, Value>,
        assign: Option<&Assign>,
    ) -> Result<(), DirectiveError> {
        let registry: Table = self.lua.globals().get("_loom").map_err(map_lua_err)?;
        let handlers: Table = registry.get("handlers").map_err(map_lua_err)?;
        let func: Function = handlers.get(name).map_err(|_| {
            DirectiveError::UnknownKind(name.to_string())
        })?;

        let mut args: Vec<LuaValue> = Vec::new();
        // Spec §14.1: positional first.
        for v in positional {
            args.push(value_to_lua(&self.lua, v).map_err(map_lua_err)?);
        }
        // Trailing named table — only if there are named args, OR if
        // this is `set` (the assign payload travels through a table).
        if !named.is_empty() || assign.is_some() {
            let t = self.lua.create_table().map_err(map_lua_err)?;
            for (k, v) in named {
                t.set(
                    k.as_str(),
                    value_to_lua(&self.lua, v).map_err(map_lua_err)?,
                )
                .map_err(map_lua_err)?;
            }
            if let Some(a) = assign {
                t.set("path", a.path.join(".")).map_err(map_lua_err)?;
                t.set(
                    "op",
                    match a.op {
                        AssignOp::Set => "=",
                        AssignOp::AddAssign => "+=",
                        AssignOp::SubAssign => "-=",
                        AssignOp::MulAssign => "*=",
                        AssignOp::DivAssign => "/=",
                    },
                )
                .map_err(map_lua_err)?;
            }
            args.push(LuaValue::Table(t));
        }

        let mv = MultiValue::from_iter(args);
        func.call::<MultiValue>(mv).map_err(map_lua_err)?;
        Ok(())
    }
}

fn map_lua_err(e: mlua::Error) -> DirectiveError {
    DirectiveError::BadArgs {
        kind: "<luau>".into(),
        message: e.to_string(),
    }
}

fn eval_positional(args: &[expr::Expr], world: &World) -> Result<Vec<Value>, DirectiveError> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        out.push(expr::eval(a, world, &mut |name, _args| {
            Err(ExprError::UnknownFunction(name.into()))
        })?);
    }
    Ok(out)
}

fn eval_named(
    args: &IndexMap<String, expr::Expr>,
    world: &World,
) -> Result<IndexMap<String, Value>, DirectiveError> {
    let mut out = IndexMap::new();
    for (k, v) in args {
        out.insert(
            k.clone(),
            expr::eval(v, world, &mut |name, _args| {
                Err(ExprError::UnknownFunction(name.into()))
            })?,
        );
    }
    Ok(out)
}

fn value_to_lua(lua: &Lua, v: &Value) -> mlua::Result<LuaValue> {
    Ok(match v {
        Value::Null => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Number(n) => LuaValue::Number(*n),
        Value::String(s) => LuaValue::String(lua.create_string(s)?),
        Value::List(items) => {
            let t = lua.create_table()?;
            for (i, it) in items.iter().enumerate() {
                t.set(i + 1, value_to_lua(lua, it)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

fn lua_to_value(v: &LuaValue) -> Value {
    match v {
        LuaValue::Nil => Value::Null,
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::Number(*i as f64),
        LuaValue::Number(n) => Value::Number(*n),
        LuaValue::String(s) => Value::String(s.to_str().map(|s| s.to_string()).unwrap_or_default()),
        LuaValue::Table(t) => {
            let mut items = Vec::new();
            let len = t.raw_len();
            for i in 1..=len {
                if let Ok(item) = t.get::<LuaValue>(i) {
                    items.push(lua_to_value(&item));
                }
            }
            Value::List(items)
        }
        _ => Value::Null,
    }
}

/// Install the `loom` global table + the `_loom` internal registry +
/// the `directive` helper.
fn install_loom_global(lua: &Lua, slot: &Slot) -> Result<(), DirectiveError> {
    let globals = lua.globals();

    // _loom.handlers — the actual dispatch table.
    let internal = lua.create_table().map_err(map_lua_err)?;
    let handlers = lua.create_table().map_err(map_lua_err)?;
    internal.set("handlers", handlers).map_err(map_lua_err)?;
    globals.set("_loom", internal).map_err(map_lua_err)?;

    let loom = lua.create_table().map_err(map_lua_err)?;

    // loom.world_get(path) -> value
    {
        let slot = slot.clone();
        let f = lua
            .create_function(move |lua, path: String| {
                let g = slot.borrow();
                let Some(ds) = g.as_ref() else {
                    return Ok(LuaValue::Nil);
                };
                // SAFETY: slot is populated only during dispatch().
                let world = unsafe { ds.world() };
                let v = world.get(&path);
                value_to_lua(lua, &v).or(Ok(LuaValue::Nil))
            })
            .map_err(map_lua_err)?;
        loom.set("world_get", f).map_err(map_lua_err)?;
    }

    // loom.world_set(path, value)
    {
        let slot = slot.clone();
        let f = lua
            .create_function(move |_, (path, value): (String, LuaValue)| {
                let g = slot.borrow();
                let Some(ds) = g.as_ref() else {
                    return Ok(());
                };
                let world = unsafe { ds.world() };
                world.set(path, lua_to_value(&value));
                Ok(())
            })
            .map_err(map_lua_err)?;
        loom.set("world_set", f).map_err(map_lua_err)?;
    }

    // loom.world_apply(path, op, rhs) — used by the set builtin.
    {
        let slot = slot.clone();
        let f = lua
            .create_function(move |_, (path, op, rhs): (String, String, LuaValue)| {
                let g = slot.borrow();
                let Some(ds) = g.as_ref() else {
                    return Ok(());
                };
                let world = unsafe { ds.world() };
                let ledger = unsafe { ds.ledger() };
                let rhs_v = lua_to_value(&rhs);
                let new_value = match op.as_str() {
                    "=" => rhs_v,
                    "+=" => arith(world.get(&path), rhs_v, |a, b| a + b),
                    "-=" => arith(world.get(&path), rhs_v, |a, b| a - b),
                    "*=" => arith(world.get(&path), rhs_v, |a, b| a * b),
                    "/=" => arith(world.get(&path), rhs_v, |a, b| a / b),
                    _ => rhs_v,
                };
                world.set(path.clone(), new_value.clone());
                ledger.push(Event::WorldSet {
                    path,
                    value: new_value.display(),
                });
                Ok(())
            })
            .map_err(map_lua_err)?;
        loom.set("world_apply", f).map_err(map_lua_err)?;
    }

    // loom.append_event(name, payload_table)
    {
        let slot = slot.clone();
        let f = lua
            .create_function(move |_, (name, payload): (String, Option<Table>)| {
                let g = slot.borrow();
                let Some(ds) = g.as_ref() else {
                    return Ok(());
                };
                let ledger = unsafe { ds.ledger() };
                let mut entries: Vec<(String, String)> = Vec::new();
                if let Some(t) = payload {
                    for pair in t.pairs::<LuaValue, LuaValue>() {
                        let (k, v) = pair.map_err(mlua::Error::external)?;
                        let key = match k {
                            LuaValue::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                            other => format!("{other:?}"),
                        };
                        entries.push((key, lua_to_value(&v).display()));
                    }
                }
                ledger.push(Event::Fired {
                    name,
                    payload: entries,
                });
                Ok(())
            })
            .map_err(map_lua_err)?;
        loom.set("append_event", f).map_err(map_lua_err)?;
    }

    // loom.suppress() — mark the current dispatch as "I already pushed
    // my own envelope, please don't add a generic one".
    {
        let slot = slot.clone();
        let f = lua
            .create_function(move |_, ()| {
                if let Some(ds) = slot.borrow_mut().as_mut() {
                    ds.suppress = true;
                }
                Ok(())
            })
            .map_err(map_lua_err)?;
        loom.set("suppress", f).map_err(map_lua_err)?;
    }

    globals.set("loom", loom).map_err(map_lua_err)?;

    // The `directive name(args) body end` helper. Spec §14 shows
    // user code declaring directives with the `directive` keyword;
    // we desugar that to `_loom.handlers[name] = fn`.
    let directive_helper = lua
        .load(
            r#"
                function directive(name, fn)
                    _loom.handlers[name] = fn
                end
            "#,
        )
        .set_name("loom:directive-helper");
    directive_helper.exec().map_err(map_lua_err)?;

    Ok(())
}

fn arith(lhs: Value, rhs: Value, op: fn(f64, f64) -> f64) -> Value {
    let l = lhs.as_number().unwrap_or(0.0);
    let r = rhs.as_number().unwrap_or(0.0);
    Value::Number(op(l, r))
}

// ---------------------------------------------------------------------
// Core builtins (§14)
// ---------------------------------------------------------------------

/// Install the core directives every Loom project gets out of the box:
/// `sfx`, `cue`, `pause`, `anchor`, `fire`, `set`, `spawn`, `cancel`,
/// `goal`, `broadcast`, `enroll`, `goto`, `compose`, `heal`, `flash`.
///
/// The audio / lighting / ledger side effects are implemented as the
/// Rust closures below. The Luau function is a thin shim that pulls
/// args out of the trailing table and calls the matching `loom.*`
/// helper, so extension authors can override one directive without
/// rewriting the others.
pub fn register_core_builtins(reg: &mut LuauRegistry) -> Result<(), DirectiveError> {
    // Generic envelope handlers — leave the surface untouched and let
    // the dispatcher add the generic `Event::Directive` envelope.
    // (`set`, `fire`, `pause`, `anchor` are syntactic forms — they live
    // in the trait-object registry and never reach Luau.)
    for name in [
        "sfx", "cue", "spawn", "cancel", "goal", "broadcast", "enroll", "goto", "compose", "heal",
        "flash",
    ] {
        reg.register_rust(name, |_, _: Variadic<LuaValue>| Ok(LuaValue::Nil))?;
    }

    // `_fire` (Lua-side helper; the directive form `<fire: …>` is
    // handled by the trait-object FireHandler).
    reg.register_rust("_fire", |lua, args: Variadic<LuaValue>| {
        let mut iter = args.into_iter();
        let first = iter.next().unwrap_or(LuaValue::Nil);
        let name = match &first {
            LuaValue::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            other => format!("{other:?}"),
        };
        let payload = iter.next();
        let payload_table = match payload {
            Some(LuaValue::Table(t)) => Some(t),
            _ => None,
        };
        let append: Function = lua.globals().get::<Table>("loom")?.get("append_event")?;
        append.call::<()>((name, payload_table))?;
        let suppress: Function = lua.globals().get::<Table>("loom")?.get("suppress")?;
        suppress.call::<()>(())?;
        Ok(LuaValue::Nil)
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives;

    #[test]
    fn sfx_emits_directive_envelope() {
        let reg = LuauRegistry::with_core_builtins().expect("registry");
        let call = directives::parse("sfx: bell").expect("parse");
        let mut world = World::new();
        let mut ledger = Ledger::default();
        let (outcome, pos, _named) = reg.dispatch(&call, &mut world, &mut ledger).unwrap();
        assert!(matches!(outcome, HandlerOutcome::Handled));
        assert_eq!(pos.len(), 1);
        // Generic envelope is added by the playhead, not the registry,
        // so the ledger is still empty after a `Handled` outcome.
        assert!(ledger.events().is_empty());
    }

    #[test]
    fn registry_routes_unknown_kinds_through_luau() {
        // The combined registry should dispatch `sfx` (a Luau core
        // builtin) without an explicit Rust handler.
        let registry = crate::directives::Registry::with_builtins();
        assert!(registry.contains("sfx"));
        let call = directives::parse("sfx: bell, fade: 200").expect("parse");
        let mut world = World::new();
        let mut ledger = Ledger::default();
        let (outcome, pos, named) =
            crate::directives::dispatch(&call, &registry, &mut world, &mut ledger).unwrap();
        assert!(matches!(outcome, HandlerOutcome::Handled));
        assert_eq!(pos.len(), 1);
        assert_eq!(named.len(), 1);
    }

    #[test]
    fn extension_registers_custom_directive() {
        let reg = LuauRegistry::with_core_builtins().expect("registry");
        reg.load_extension_str(
            "test",
            r#"
                directive("heal", function(target, opts)
                    loom.append_event("healed", { target = target, amount = opts.amount })
                    loom.suppress()
                end)
            "#,
        )
        .unwrap();
        assert!(reg.contains("heal"));
        let call = directives::parse("heal: 'Wren', amount: 20").expect("parse");
        let mut world = World::new();
        let mut ledger = Ledger::default();
        let (outcome, _p, _n) = reg.dispatch(&call, &mut world, &mut ledger).unwrap();
        assert!(matches!(outcome, HandlerOutcome::Suppressed));
        let evs = ledger.events();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            Event::Fired { name, payload } => {
                assert_eq!(name, "healed");
                let map: std::collections::BTreeMap<_, _> = payload.iter().cloned().collect();
                assert_eq!(map.get("target").map(|s| s.as_str()), Some("Wren"));
                assert_eq!(map.get("amount").map(|s| s.as_str()), Some("20"));
            }
            other => panic!("expected Fired, got {other:?}"),
        }
    }

    #[test]
    fn arg_convention_positional_then_table() {
        // Spec §14.1: positional first, single trailing table.
        // We assert by registering a probe that records its argv shape.
        let reg = LuauRegistry::with_core_builtins().expect("registry");
        reg.load_extension_str(
            "probe",
            r#"
                directive("probe", function(...)
                    local args = { ... }
                    local n = #args
                    local last = args[n]
                    local kind = type(last)
                    loom.append_event("probe_shape", {
                        argc = n,
                        last_kind = kind,
                        positional_first = tostring(args[1]),
                    })
                    loom.suppress()
                end)
            "#,
        )
        .unwrap();
        let call = directives::parse("probe: 'A', 'B', key: 1").expect("parse");
        let mut world = World::new();
        let mut ledger = Ledger::default();
        reg.dispatch(&call, &mut world, &mut ledger).unwrap();
        let evs = ledger.events();
        let payload = match &evs[0] {
            Event::Fired { payload, .. } => payload.clone(),
            other => panic!("{other:?}"),
        };
        let map: std::collections::BTreeMap<_, _> = payload.into_iter().collect();
        assert_eq!(map.get("argc").map(|s| s.as_str()), Some("3"));
        assert_eq!(map.get("last_kind").map(|s| s.as_str()), Some("table"));
        assert_eq!(map.get("positional_first").map(|s| s.as_str()), Some("A"));
    }

    #[test]
    fn bare_directive_no_args() {
        let reg = LuauRegistry::with_core_builtins().expect("registry");
        reg.load_extension_str(
            "probe",
            r#"
                directive("ping", function(...)
                    local args = { ... }
                    loom.append_event("ping", { argc = #args })
                    loom.suppress()
                end)
            "#,
        )
        .unwrap();
        let call = directives::parse("ping").expect("parse");
        let mut world = World::new();
        let mut ledger = Ledger::default();
        reg.dispatch(&call, &mut world, &mut ledger).unwrap();
        match &ledger.events()[0] {
            Event::Fired { payload, .. } => {
                let m: std::collections::BTreeMap<_, _> = payload.iter().cloned().collect();
                assert_eq!(m.get("argc").map(|s| s.as_str()), Some("0"));
            }
            _ => unreachable!(),
        }
    }
}
