//! Lua 5.4 IPC commands via mlua.

use mlua::{Lua, Result as LuaResult, Value};
use serde_json::Value as JsonValue;

/// Execute a Lua script and return the result as JSON.
/// Called from frontend via: invoke('lua_exec', { script, args })
pub fn lua_exec(script: &str, args: Option<&serde_json::Map<String, JsonValue>>) -> Result<JsonValue, String> {
    let lua = Lua::new();

    // Inject arguments as globals
    if let Some(args) = args {
        let globals = lua.globals();
        for (key, value) in args {
            let lua_val = json_to_lua(&lua, value).map_err(|e| e.to_string())?;
            globals.set(key.as_str(), lua_val).map_err(|e| e.to_string())?;
        }
    }

    let result: Value = lua.load(script).eval().map_err(|e| e.to_string())?;
    lua_to_json(&result).map_err(|e| e.to_string())
}

/// Convert a serde_json Value to a Lua Value.
fn json_to_lua(lua: &Lua, value: &JsonValue) -> LuaResult<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(b) => Ok(Value::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
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

/// Convert a Lua Value to a serde_json Value.
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
            // Try array first (sequential integer keys starting at 1)
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: Value = t.raw_get(i)?;
                    arr.push(lua_to_json(&v)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                // Object
                let mut map = serde_json::Map::new();
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

    #[test]
    fn test_lua_exec_simple() {
        let result = lua_exec("return 2 + 2", None).unwrap();
        assert_eq!(result, JsonValue::Number(4.into()));
    }

    #[test]
    fn test_lua_exec_with_args() {
        let mut args = serde_json::Map::new();
        args.insert("x".to_string(), JsonValue::Number(10.into()));
        args.insert("y".to_string(), JsonValue::Number(20.into()));

        let result = lua_exec("return x + y", Some(&args)).unwrap();
        assert_eq!(result, JsonValue::Number(30.into()));
    }

    #[test]
    fn test_lua_exec_string() {
        let mut args = serde_json::Map::new();
        args.insert("name".to_string(), JsonValue::String("Prism".to_string()));

        let result = lua_exec("return 'Hello, ' .. name", Some(&args)).unwrap();
        assert_eq!(result, JsonValue::String("Hello, Prism".to_string()));
    }

    #[test]
    fn test_lua_exec_table() {
        let result = lua_exec("return {a = 1, b = 2}", None).unwrap();
        let obj = result.as_object().unwrap();
        assert_eq!(obj.get("a"), Some(&JsonValue::Number(1.into())));
        assert_eq!(obj.get("b"), Some(&JsonValue::Number(2.into())));
    }

    #[test]
    fn test_lua_exec_error() {
        let result = lua_exec("error('test error')", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("test error"));
    }

    #[test]
    fn test_lua_round_trip_same_script() {
        // The same script that runs in mlua (here) should produce
        // the same result as wasmoon (browser). This test validates
        // the daemon side of the contract.
        let script = r#"
            local function factorial(n)
                if n <= 1 then return 1 end
                return n * factorial(n - 1)
            end
            return factorial(input)
        "#;

        let mut args = serde_json::Map::new();
        args.insert("input".to_string(), JsonValue::Number(5.into()));

        let result = lua_exec(script, Some(&args)).unwrap();
        assert_eq!(result, JsonValue::Number(120.into()));
    }
}
