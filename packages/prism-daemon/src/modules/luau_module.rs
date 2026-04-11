//! Luau scripting module. Exposes `luau.exec`.
//!
//! Payload shape: `{ script: String, args?: Object }`
//! Result: whatever JSON the script evaluates to.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use mlua::{Lua, MultiValue, Result as LuaResult, Value};
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value as JsonValue};

pub struct LuauModule;

impl DaemonModule for LuauModule {
    fn id(&self) -> &str {
        "prism.luau"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        builder.registry().register("luau.exec", |payload| {
            let args: ExecArgs = serde_json::from_value(payload)
                .map_err(|e| CommandError::handler("luau.exec", e.to_string()))?;
            exec(&args.script, args.args.as_ref())
                .map_err(|e| CommandError::handler("luau.exec", e))
        })?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ExecArgs {
    script: String,
    #[serde(default)]
    args: Option<JsonMap<String, JsonValue>>,
}

/// Execute a Luau script and return the result as JSON.
pub fn exec(script: &str, args: Option<&JsonMap<String, JsonValue>>) -> Result<JsonValue, String> {
    let lua = Lua::new();

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

fn json_to_lua(lua: &Lua, value: &JsonValue) -> LuaResult<Value> {
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

fn lua_to_json(value: &Value) -> LuaResult<JsonValue> {
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
    fn luau_pure_fn_still_usable() {
        // The free function remains the hot path for transport adapters
        // that don't want JSON intermediation (e.g. Tauri).
        let result = exec("return 2 * 3", None).unwrap();
        assert_eq!(result, JsonValue::Number(6.into()));
    }
}
