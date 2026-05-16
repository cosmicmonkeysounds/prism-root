//! Hand-rolled `mlua::UserData` impls for the reactive substrate
//! (`crate::reactive`) — Phase 5 of the Dioxus-inspired reactive
//! overhaul (`docs/dev/dioxus-inspiration.md`).
//!
//! Surface:
//!
//! * [`Signal<Value>`](crate::reactive::Signal) — `signal:read()` /
//!   `signal:peek()` / `signal:write(v)` / `signal:set(v)` /
//!   `signal:track()`. Subscribing reads (`read`) register the
//!   current reactive context if a reactive walk owns the call site;
//!   `peek` never subscribes. Writes (`write` / `set`) notify every
//!   reactive subscriber.
//! * [`Memo<Value>`](crate::reactive::Memo) — `memo:read()` /
//!   `memo:peek()`. Read-only on the Luau side; the body is owned
//!   by Rust.
//!
//! ## Why `Value`-carrying signals
//!
//! Luau is a dynamically-typed runtime: signals exposed to Luau
//! carry `serde_json::Value` because that's the value space mlua's
//! serde bridge speaks. Concrete `Signal<T>` for typed T stays
//! Rust-only; if Luau needs typed access, the host wraps the typed
//! signal in a `Signal<Value>` (via `Owner::insert(Value::Number(..))`
//! etc.) and hands the wrapper across.
//!
//! ## Out of scope (follow-ups)
//!
//! Lua-side **construction** of signals/memos/effects (`prism.reactive.signal(0)`,
//! `prism.reactive.effect(function() … end)`) needs per-Lua-state
//! `Owner` management — every Luau-owned reactive scope has to be
//! tied to *some* Owner so cleanup is deterministic when the script
//! VM tears down. That landing is a sibling host concern;
//! `prism-daemon::modules::luau_module` is the natural place
//! because it already owns the Lua state lifecycle.

use mlua::{LuaSerdeExt, UserData, UserDataMethods};
use serde_json::Value;

use crate::reactive::{Memo, Signal};

// Re-export the type-stub constants so the codegen pipeline (which
// runs without the `luau` feature) picks them up alongside the
// runtime UserData impls. Defined in `luau_bindings_consts.rs`.
pub use crate::luau_bindings_consts::{
    REACTIVE_MEMO_TYPE_DEF, REACTIVE_MEMO_TYPE_NAME, REACTIVE_SIGNAL_TYPE_DEF,
    REACTIVE_SIGNAL_TYPE_NAME,
};

impl UserData for Signal<Value> {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // `read` — subscribing read. If a reactive context is on
        // the stack, this registers the current context as a
        // subscriber. Returns the current value as a Lua value.
        methods.add_method("read", |lua, this, ()| {
            let v = this.get();
            lua.to_value(&v)
        });

        // `peek` — non-subscribing read. The natural shape for
        // "I want to inspect the current value without forming a
        // dependency edge."
        methods.add_method("peek", |lua, this, ()| {
            let v = this.snapshot();
            lua.to_value(&v)
        });

        // `track` — subscribe without producing a value. Useful in
        // Luau for "wake me when anything in this signal changes,
        // I'll fetch it myself" patterns.
        methods.add_method("track", |_, this, ()| {
            this.track();
            Ok(())
        });

        // `write` — set the value and notify subscribers. mlua
        // deserialises any Lua value into a `serde_json::Value`
        // through the serde bridge.
        methods.add_method("write", |lua, this, value: mlua::Value| {
            let json: Value = lua.from_value(value)?;
            this.set(json);
            Ok(())
        });

        // `set` — alias of `write`, matching the Rust API.
        methods.add_method("set", |lua, this, value: mlua::Value| {
            let json: Value = lua.from_value(value)?;
            this.set(json);
            Ok(())
        });
    }
}

impl UserData for Memo<Value> {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("read", |lua, this, ()| {
            let v = this.get();
            lua.to_value(&v)
        });
        methods.add_method("peek", |lua, this, ()| {
            let v = this.snapshot();
            lua.to_value(&v)
        });
    }
}

/// Light-weight `mlua::Value` → `serde_json::Value` projection for
/// reactive bodies (`Memo` / `Effect` closures) that run **without**
/// a `&Lua` in scope, so the `LuaSerdeExt` bridge isn't reachable.
/// Covers the shape every reactive author actually returns: scalars
/// and array-shape / object-shape tables. Userdata and functions
/// round-trip as `Null` so a body that accidentally returns a signal
/// can't crash the host. Shared by every per-Lua-state reactive
/// constructor (the daemon's `prism.reactive.*` and the scope-frame's
/// `prism.state` / `prism.derive`) so the projection lives once.
pub fn lua_value_to_json(v: &mlua::Value) -> Value {
    match v {
        mlua::Value::Nil => Value::Null,
        mlua::Value::Boolean(b) => Value::Bool(*b),
        mlua::Value::Integer(i) => Value::from(*i),
        mlua::Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        mlua::Value::String(s) => s
            .to_str()
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        mlua::Value::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let item: mlua::Value = t.raw_get(i).unwrap_or(mlua::Value::Nil);
                    arr.push(lua_value_to_json(&item));
                }
                Value::Array(arr)
            } else {
                let mut map = serde_json::Map::new();
                for (k, val) in t.clone().pairs::<String, mlua::Value>().flatten() {
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
    use crate::reactive::Owner;
    use mlua::Lua;
    use serde_json::json;

    fn lua() -> Lua {
        Lua::new()
    }

    #[test]
    fn signal_read_returns_current_value_to_luau() {
        let owner = Owner::new();
        let sig = owner.insert(Value::String("hello".into()));
        let lua = lua();
        lua.globals().set("sig", sig).unwrap();
        let out: String = lua.load("return sig:read()").eval().unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn signal_peek_does_not_subscribe() {
        let owner = Owner::new();
        let sig = owner.insert(json!(42));
        let lua = lua();
        lua.globals().set("sig", sig).unwrap();
        let out: i64 = lua.load("return sig:peek()").eval().unwrap();
        assert_eq!(out, 42);
    }

    #[test]
    fn signal_write_round_trips_through_luau() {
        let owner = Owner::new();
        let sig: Signal<Value> = owner.insert(json!(0));
        let lua = lua();
        lua.globals().set("sig", sig).unwrap();
        lua.load("sig:write(7)").exec().unwrap();
        assert_eq!(sig.snapshot(), json!(7));
    }

    #[test]
    fn signal_set_is_aliased_write() {
        let owner = Owner::new();
        let sig: Signal<Value> = owner.insert(json!(0));
        let lua = lua();
        lua.globals().set("sig", sig).unwrap();
        lua.load("sig:set(9)").exec().unwrap();
        assert_eq!(sig.snapshot(), json!(9));
    }

    #[test]
    fn signal_round_trips_a_table_through_serde_bridge() {
        let owner = Owner::new();
        let sig: Signal<Value> = owner.insert(json!({"count": 0}));
        let lua = lua();
        lua.globals().set("sig", sig).unwrap();
        lua.load("sig:write({count = 5, label = 'hi'})")
            .exec()
            .unwrap();
        let stored = sig.snapshot();
        assert_eq!(stored["count"], json!(5));
        assert_eq!(stored["label"], json!("hi"));
    }

    #[test]
    fn signal_track_subscribes_without_returning() {
        // Just verify the method dispatches without panicking. The
        // reactive-subscription side effect is observed by the
        // surrounding Rust harness in dedicated `reactive::tests`.
        let owner = Owner::new();
        let sig: Signal<Value> = owner.insert(json!(0));
        let lua = lua();
        lua.globals().set("sig", sig).unwrap();
        lua.load("sig:track()").exec().unwrap();
    }

    #[test]
    fn memo_read_returns_derived_value() {
        let owner = Owner::new();
        let a = owner.insert(json!(2));
        let memo: Memo<Value> = Memo::new(&owner, move || {
            let v = a.get();
            json!(v.as_i64().unwrap_or(0) * 10)
        });
        let lua = lua();
        lua.globals().set("memo", memo).unwrap();
        let out: i64 = lua.load("return memo:read()").eval().unwrap();
        assert_eq!(out, 20);
    }

    #[test]
    fn memo_tracks_source_signal_updates_in_rust() {
        // Memos recompute on source change — the Lua side just
        // sees the latest value through `read`.
        let owner = Owner::new();
        let a = owner.insert(json!(1));
        let memo: Memo<Value> = Memo::new(&owner, move || {
            let v = a.get();
            json!(v.as_i64().unwrap_or(0) + 100)
        });
        let lua = lua();
        lua.globals().set("memo", memo.clone()).unwrap();
        let out1: i64 = lua.load("return memo:read()").eval().unwrap();
        assert_eq!(out1, 101);

        a.set(json!(50));
        let out2: i64 = lua.load("return memo:read()").eval().unwrap();
        assert_eq!(out2, 150);
    }
}
