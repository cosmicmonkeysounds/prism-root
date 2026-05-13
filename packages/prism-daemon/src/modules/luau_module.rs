//! Luau scripting module. Exposes `luau.exec` and `luau.eval`.
//!
//! `luau.exec` payload: `{ script: String, args?: Object }` — runs the
//! script body and returns whatever its last `return` statement yields.
//!
//! `luau.eval` payload: `{ expression: String, args?: Object }` — wraps
//! the input in `return (<expression>)` so palette REPL authors can
//! type `prism.objects:query{}` and get the value back without writing
//! `return` themselves (Phase 4.7 of
//! `docs/dev/luau-integration-plan.md`).
//!
//! Every script runs with the `prism` global pre-installed (see
//! [`crate::modules::prism_context::PrismContext`]) — Phase 1 of
//! `docs/dev/luau-integration-plan.md`. Today that surfaces design
//! tokens + shell-mode tag; later phases bolt object/document/signal
//! access onto the same userdata without changing this entry point.

use crate::modules::prism_context::{self, PrismContext};
use mlua::{Lua, MultiValue, Result as LuaResult, Value};
use prism_luau_derive::{daemon_command, daemon_module};
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value as JsonValue};

#[daemon_module(id = "prism.luau", commands(exec_cmd, eval_cmd))]
pub struct LuauModule;

#[derive(Debug, Deserialize)]
struct ExecArgs {
    script: String,
    #[serde(default)]
    args: Option<JsonMap<String, JsonValue>>,
}

#[daemon_command(id = "luau.exec")]
fn exec_cmd(args: ExecArgs) -> Result<JsonValue, String> {
    exec(&args.script, args.args.as_ref())
}

#[derive(Debug, Deserialize)]
struct EvalArgs {
    expression: String,
    #[serde(default)]
    args: Option<JsonMap<String, JsonValue>>,
}

#[daemon_command(id = "luau.eval")]
fn eval_cmd(args: EvalArgs) -> Result<JsonValue, String> {
    eval(&args.expression, args.args.as_ref())
}

/// Evaluate a Luau expression and return its value as JSON. Wraps
/// `expression` in `return (...)` so REPL authors don't need to type
/// the `return` keyword themselves. Multi-statement bodies that need a
/// trailing expression should call [`exec`] instead and supply their
/// own `return`.
pub fn eval(
    expression: &str,
    args: Option<&JsonMap<String, JsonValue>>,
) -> Result<JsonValue, String> {
    let wrapped = format!("return ({expression})");
    exec(&wrapped, args)
}

/// Execute a Luau script and return the result as JSON. Equivalent to
/// [`exec_with_context`] called with `PrismContext::default()`.
pub fn exec(script: &str, args: Option<&JsonMap<String, JsonValue>>) -> Result<JsonValue, String> {
    exec_with_context(script, args, PrismContext::default())
}

/// Execute a Luau script with a host-supplied [`PrismContext`]. The
/// daemon's `luau.exec` command always uses the default context;
/// hosts that have their own design tokens / shell mode override
/// (Studio, the relay's render path) reach for this entry point so
/// scripts see the live values instead of the boot defaults.
pub fn exec_with_context(
    script: &str,
    args: Option<&JsonMap<String, JsonValue>>,
    ctx: PrismContext,
) -> Result<JsonValue, String> {
    exec_with_setup(script, args, ctx, |_| Ok(()))
}

/// Execute a Luau script with a host-supplied [`PrismContext`] and a
/// `setup` closure that runs against the freshly-built [`Lua`] state
/// after the `prism` global is installed but before the script runs.
/// The shell uses this to install Phase-5 handles (`prism.document` /
/// `prism.signals` / `prism.selection` / `prism.app`) on top of the
/// daemon's stateless context.
pub fn exec_with_setup<F>(
    script: &str,
    args: Option<&JsonMap<String, JsonValue>>,
    ctx: PrismContext,
    setup: F,
) -> Result<JsonValue, String>
where
    F: FnOnce(&Lua) -> mlua::Result<()>,
{
    let lua = Lua::new();

    prism_context::install(&lua, ctx).map_err(|e| e.to_string())?;

    // Phase 5 completion of `docs/dev/dioxus-inspiration.md`: every
    // Luau script runs with `prism.reactive.signal` /
    // `.memo` / `.effect` / `.batch` constructors installed against a
    // per-state `Owner`. The owner drops with the Lua state at the
    // end of `exec_with_setup`, so a script's reactive scopes never
    // leak past its VM lifetime.
    crate::modules::luau_reactive::install(&lua).map_err(|e| e.to_string())?;

    setup(&lua).map_err(|e| e.to_string())?;

    if let Some(args) = args {
        let globals = lua.globals();
        for (key, value) in args {
            let lua_val = json_to_lua(&lua, value).map_err(|e| e.to_string())?;
            globals
                .set(key.as_str(), lua_val)
                .map_err(|e| e.to_string())?;
        }
    }

    let results: MultiValue = lua
        .load(script)
        .into_function()
        .map_err(|e| e.to_string())?
        .call(())
        .map_err(|e| e.to_string())?;
    let result = results.into_iter().next().unwrap_or(Value::Nil);
    lua_to_json(&result).map_err(|e| e.to_string())
}

pub fn json_to_lua(lua: &Lua, value: &JsonValue) -> LuaResult<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(b) => Ok(Value::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        JsonValue::String(s) => Ok(Value::String(lua.create_string(s)?)),
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

pub fn lua_to_json(value: &Value) -> LuaResult<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
        Value::Integer(i) => Ok(JsonValue::Number((*i).into())),
        Value::Number(f) => Ok(serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: Value = t.raw_get(i)?;
                    arr.push(lua_to_json(&v)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                let mut map = JsonMap::new();
                for pair in t.clone().pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_json(&v)?);
                }
                Ok(JsonValue::Object(map))
            }
        }
        _ => Ok(JsonValue::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use crate::registry::CommandError;
    use serde_json::json;

    #[test]
    fn luau_module_registers_exec() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        assert!(kernel.capabilities().contains(&"luau.exec".to_string()));
    }

    #[test]
    fn luau_exec_simple_expression() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke("luau.exec", json!({ "script": "return 2 + 2" }))
            .unwrap();
        assert_eq!(out, JsonValue::Number(4.into()));
    }

    #[test]
    fn luau_exec_with_named_args() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke(
                "luau.exec",
                json!({ "script": "return x + y", "args": { "x": 10, "y": 20 } }),
            )
            .unwrap();
        assert_eq!(out, JsonValue::Number(30.into()));
    }

    #[test]
    fn luau_exec_error_surfaces_as_command_error() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let err = kernel
            .invoke("luau.exec", json!({ "script": "error('boom')" }))
            .unwrap_err();
        match err {
            CommandError::Handler { command, message } => {
                assert_eq!(command, "luau.exec");
                assert!(message.contains("boom"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn luau_exec_exposes_prism_global() {
        // Phase 1 of the Luau integration plan: every script sees the
        // `prism` global without any opt-in. Reading the default
        // accent-red channel proves the userdata + nested-getter chain
        // round-trips through the kernel command surface.
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke(
                "luau.exec",
                json!({ "script": "return prism.tokens.colors.accent.r" }),
            )
            .unwrap();
        assert_eq!(out, JsonValue::Number(110.into()));
    }

    #[test]
    fn luau_exec_exposes_shell_mode_as_string() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke("luau.exec", json!({ "script": "return prism.shell_mode" }))
            .unwrap();
        assert_eq!(out, JsonValue::String("Build".to_string()));
    }

    #[test]
    fn luau_pure_fn_still_usable() {
        // The free function remains the hot path for transport adapters
        // that don't want JSON intermediation (e.g. the Studio IPC bridge).
        let result = exec("return 2 * 3", None).unwrap();
        assert_eq!(result, JsonValue::Number(6.into()));
    }

    #[test]
    fn luau_eval_returns_expression_value() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke("luau.eval", json!({ "expression": "2 + 2" }))
            .unwrap();
        assert_eq!(out, JsonValue::Number(4.into()));
    }

    #[test]
    fn luau_eval_sees_prism_global() {
        // Phase 4.7: REPL authors expect the same context as scripts.
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke(
                "luau.eval",
                json!({ "expression": "prism.tokens.colors.accent.r" }),
            )
            .unwrap();
        assert_eq!(out, JsonValue::Number(110.into()));
    }

    #[test]
    fn luau_eval_with_args() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        let out = kernel
            .invoke(
                "luau.eval",
                json!({ "expression": "x * y", "args": { "x": 6, "y": 7 } }),
            )
            .unwrap();
        assert_eq!(out, JsonValue::Number(42.into()));
    }

    #[test]
    fn luau_module_registers_eval() {
        let kernel = DaemonBuilder::new().with_luau().build().unwrap();
        assert!(kernel.capabilities().contains(&"luau.eval".to_string()));
    }

    #[test]
    fn luau_eval_pure_fn() {
        let out = eval("1 + 2 + 3", None).unwrap();
        assert_eq!(out, JsonValue::Number(6.into()));
    }
}
