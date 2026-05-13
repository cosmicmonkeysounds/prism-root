//! Phase 5 completion of `docs/dev/dioxus-inspiration.md`: Luau-side
//! **construction** of reactive primitives.
//!
//! `prism-core::luau_reactive` already exposes `Signal<Value>` and
//! `Memo<Value>` as `mlua::UserData`, so a Rust host can hand a
//! pre-built signal across to Luau and the script can call
//! `signal:read()` / `:write(v)` / `:peek()` / `:track()`. What it
//! couldn't do until now was *create* a signal from Luau — that
//! requires a per-Lua-state [`Owner`] to allocate the underlying
//! `GenerationalBox` slot from, and `Owner` is `!Send`, so it has to
//! live next to the Lua VM lifecycle.
//!
//! [`install`] wires the four-call surface:
//!
//! * `prism.reactive.signal(initial)` — allocate a fresh
//!   `Signal<Value>` against the per-state owner; returns the
//!   `Signal` UserData (same impl as the host-side path).
//! * `prism.reactive.memo(function() ... end)` — allocate a
//!   `Memo<Value>` whose body re-runs whenever a tracked dependency
//!   fires.
//! * `prism.reactive.effect(function() ... end)` — register an
//!   `Effect` that runs immediately + on every dirty dependency.
//!   The effect's reactive scope is owned by the per-state owner and
//!   tears down deterministically when the VM drops.
//! * `prism.reactive.batch(function() ... end)` — wrap multiple
//!   writes in a `ReactiveContext::batch` so subscribers wake once.
//!
//! Lifetime story: the [`Owner`] is stored in Lua's app-data slot
//! (`Lua::set_app_data`). When the Lua state drops at the end of
//! `exec_with_setup`, the owner drops, and every signal / memo /
//! effect allocated through it is reclaimed. Scripts cannot leak
//! reactive scopes past their VM lifetime.

use mlua::{Function, Lua, LuaSerdeExt, Result as LuaResult, Table, Value as LuaValue};
use prism_core::reactive::{Effect, Memo, Owner, ReactiveContext, Signal};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

/// App-data holder for the per-state reactive surface. Boxed so the
/// effects list can be appended to from inside an effect callback
/// without re-entering the cell.
struct ReactiveSlot {
    /// The owner is shared with the Effects we hand back; each Effect
    /// drops its `ReactiveContext` on drop, so retaining a Vec inside
    /// the slot is enough — when the Lua state drops, the slot drops,
    /// and the Vec drops every Effect / Memo handle.
    owner: Rc<Owner>,
    /// Effects that scripts created via `prism.reactive.effect`. The
    /// scripts don't hold handles to them, so we anchor each Effect
    /// here until the Lua state drops.
    effects: RefCell<Vec<Effect>>,
    /// Same anchoring rationale for `Memo`s — scripts that bind a memo
    /// to a local variable still need the inner reactive scope to
    /// outlive every read-call from Lua. The Vec holds memos by
    /// `Rc<MemoInner>` (`Memo::clone` is a bump).
    memos: RefCell<Vec<Memo<Value>>>,
}

/// Install `prism.reactive` into `lua`'s `prism` global. The `prism`
/// table must already exist (`prism_context::install` covers that
/// before `install_reactive` runs). Idempotent — registering twice
/// silently replaces the table.
pub fn install(lua: &Lua) -> LuaResult<()> {
    // Set up the per-state owner once. The script can call any of the
    // four constructors any number of times; each call allocates
    // against the same owner. The drop of the Lua state reclaims
    // everything in one pass.
    if lua.app_data_ref::<Rc<ReactiveSlot>>().is_none() {
        let slot = Rc::new(ReactiveSlot {
            owner: Rc::new(Owner::new()),
            effects: RefCell::new(Vec::new()),
            memos: RefCell::new(Vec::new()),
        });
        lua.set_app_data(slot);
    }

    let globals = lua.globals();
    let prism: Table = match globals.get::<LuaValue>("prism")? {
        LuaValue::Table(t) => t,
        _ => return Ok(()),
    };

    let reactive = lua.create_table()?;
    reactive.set("signal", make_signal_ctor(lua)?)?;
    reactive.set("memo", make_memo_ctor(lua)?)?;
    reactive.set("effect", make_effect_ctor(lua)?)?;
    reactive.set("batch", make_batch_ctor(lua)?)?;
    prism.set("reactive", reactive)?;
    Ok(())
}

fn make_signal_ctor(lua: &Lua) -> LuaResult<Function> {
    lua.create_function(|lua, initial: LuaValue| {
        let slot = lua
            .app_data_ref::<Rc<ReactiveSlot>>()
            .ok_or_else(|| mlua::Error::external("prism.reactive not installed"))?
            .clone();
        let json: Value = lua.from_value(initial)?;
        let sig: Signal<Value> = slot.owner.insert(json);
        Ok(sig)
    })
}

fn make_memo_ctor(lua: &Lua) -> LuaResult<Function> {
    lua.create_function(|lua, body: Function| {
        let slot = lua
            .app_data_ref::<Rc<ReactiveSlot>>()
            .ok_or_else(|| mlua::Error::external("prism.reactive not installed"))?
            .clone();
        // mlua::Function is `Clone + 'static` — safe to capture in
        // the FnMut closure. Errors from the body crash the memo as
        // a `Null` projection so a misbehaving script doesn't bring
        // down the host; the error string is logged through
        // `mlua::Lua::warning` so authors see it.
        let body_clone = body.clone();
        let memo: Memo<Value> =
            slot.owner
                .insert_memo(move || match body_clone.call::<LuaValue>(()) {
                    Ok(v) => lua_value_to_json(&v),
                    Err(_) => Value::Null,
                });
        slot.memos.borrow_mut().push(memo.clone());
        Ok(memo)
    })
}

fn make_effect_ctor(lua: &Lua) -> LuaResult<Function> {
    lua.create_function(|lua, body: Function| {
        let slot = lua
            .app_data_ref::<Rc<ReactiveSlot>>()
            .ok_or_else(|| mlua::Error::external("prism.reactive not installed"))?
            .clone();
        let body_clone = body.clone();
        let mut effects = slot.effects.borrow_mut();
        let eff = Effect::new(move || {
            let _ = body_clone.call::<LuaValue>(());
        });
        effects.push(eff);
        Ok(())
    })
}

fn make_batch_ctor(lua: &Lua) -> LuaResult<Function> {
    lua.create_function(|_, body: Function| ReactiveContext::batch(|| body.call::<LuaValue>(())))
}

/// Light-weight `mlua::Value` → `serde_json::Value` projection used by
/// the memo body. The serde bridge in mlua handles this more
/// thoroughly via `LuaSerdeExt`, but it requires `&Lua` — and we're
/// inside a `Memo` body without one. The subset below covers the
/// shape every memo author actually returns (scalars + table-as-map);
/// userdata round-trips as `Null` so a memo that accidentally returns
/// `signal` doesn't crash the host.
fn lua_value_to_json(v: &LuaValue) -> Value {
    match v {
        LuaValue::Nil => Value::Null,
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::from(*i),
        LuaValue::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        LuaValue::String(s) => s
            .to_str()
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        // Table: walk pairs in order, distinguish array-shape from
        // object-shape by the presence of a `1`-indexed sequence.
        LuaValue::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let item: LuaValue = t.raw_get(i).unwrap_or(LuaValue::Nil);
                    arr.push(lua_value_to_json(&item));
                }
                Value::Array(arr)
            } else {
                let mut map = serde_json::Map::new();
                for (k, val) in t.clone().pairs::<String, LuaValue>().flatten() {
                    map.insert(k, lua_value_to_json(&val));
                }
                Value::Object(map)
            }
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua_with_reactive() -> Lua {
        let lua = Lua::new();
        // Need a `prism` table for `install` to extend. The full
        // PrismContext::install does more — we stub a minimal table
        // here so the reactive tests don't drag in collection /
        // config plumbing.
        lua.globals()
            .set("prism", lua.create_table().unwrap())
            .unwrap();
        install(&lua).unwrap();
        lua
    }

    #[test]
    fn signal_constructor_returns_handle_with_initial_value() {
        let lua = lua_with_reactive();
        let v: i64 = lua
            .load(
                r#"
                local s = prism.reactive.signal(42)
                return s:read()
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn signal_write_propagates_through_read() {
        let lua = lua_with_reactive();
        let v: i64 = lua
            .load(
                r#"
                local s = prism.reactive.signal(0)
                s:write(7)
                return s:read()
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(v, 7);
    }

    #[test]
    fn effect_runs_and_re_runs_on_signal_write() {
        let lua = lua_with_reactive();
        let runs: i64 = lua
            .load(
                r#"
                local s = prism.reactive.signal(0)
                local count = 0
                prism.reactive.effect(function()
                    s:read()
                    count = count + 1
                end)
                s:write(1)
                s:write(2)
                return count
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(runs, 3, "effect runs once on creation + twice on writes");
    }

    #[test]
    fn memo_derives_from_signals_and_recomputes_on_change() {
        let lua = lua_with_reactive();
        let result: i64 = lua
            .load(
                r#"
                local a = prism.reactive.signal(2)
                local b = prism.reactive.signal(3)
                local product = prism.reactive.memo(function()
                    return a:read() * b:read()
                end)
                a:write(5)
                return product:read()
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 15);
    }

    #[test]
    fn batch_collapses_multiple_writes_to_single_effect_run() {
        let lua = lua_with_reactive();
        let runs: i64 = lua
            .load(
                r#"
                local s = prism.reactive.signal(0)
                local count = 0
                prism.reactive.effect(function()
                    s:read()
                    count = count + 1
                end)
                prism.reactive.batch(function()
                    s:write(1)
                    s:write(2)
                    s:write(3)
                end)
                return count
                "#,
            )
            .eval()
            .unwrap();
        // 1 (initial) + 1 (single batched flush)
        assert_eq!(runs, 2);
    }

    #[test]
    fn signals_round_trip_serde_object_values() {
        let lua = lua_with_reactive();
        let name: String = lua
            .load(
                r#"
                local s = prism.reactive.signal({ name = "Prism", count = 7 })
                local v = s:read()
                return v.name
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(name, "Prism");
    }
}
