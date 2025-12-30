//! Cache API for Lua plugins
//!
//! Provides in-memory caching with optional TTL:
//! - `owlry.cache.get(key)` - Get cached value
//! - `owlry.cache.set(key, value, ttl_seconds?)` - Set cached value
//! - `owlry.cache.delete(key)` - Delete cached value
//! - `owlry.cache.clear()` - Clear all cached values

use mlua::{Lua, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Cached entry with optional expiration
struct CacheEntry {
    value: String, // Store as JSON string for simplicity
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.expires_at.map(|e| Instant::now() > e).unwrap_or(false)
    }
}

/// Global cache storage (shared across all plugins)
static CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register cache APIs
pub fn register_cache_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let cache_table = lua.create_table()?;

    // owlry.cache.get(key) -> value or nil
    cache_table.set(
        "get",
        lua.create_function(|lua, key: String| {
            let cache = CACHE.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock cache: {}", e))
            })?;

            if let Some(entry) = cache.get(&key) {
                if entry.is_expired() {
                    drop(cache);
                    // Remove expired entry
                    if let Ok(mut cache) = CACHE.lock() {
                        cache.remove(&key);
                    }
                    return Ok(Value::Nil);
                }

                // Parse JSON back to Lua value
                let json_value: serde_json::Value = serde_json::from_str(&entry.value)
                    .map_err(|e| mlua::Error::external(format!("Failed to parse cached value: {}", e)))?;

                json_to_lua(lua, &json_value)
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;

    // owlry.cache.set(key, value, ttl_seconds?) -> boolean
    cache_table.set(
        "set",
        lua.create_function(|_lua, (key, value, ttl): (String, Value, Option<u64>)| {
            let json_value = lua_value_to_json(&value)?;
            let json_str = serde_json::to_string(&json_value)
                .map_err(|e| mlua::Error::external(format!("Failed to serialize value: {}", e)))?;

            let expires_at = ttl.map(|secs| Instant::now() + Duration::from_secs(secs));

            let entry = CacheEntry {
                value: json_str,
                expires_at,
            };

            let mut cache = CACHE.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock cache: {}", e))
            })?;

            cache.insert(key, entry);
            Ok(true)
        })?,
    )?;

    // owlry.cache.delete(key) -> boolean (true if key existed)
    cache_table.set(
        "delete",
        lua.create_function(|_lua, key: String| {
            let mut cache = CACHE.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock cache: {}", e))
            })?;

            Ok(cache.remove(&key).is_some())
        })?,
    )?;

    // owlry.cache.clear() -> number of entries removed
    cache_table.set(
        "clear",
        lua.create_function(|_lua, ()| {
            let mut cache = CACHE.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock cache: {}", e))
            })?;

            let count = cache.len();
            cache.clear();
            Ok(count)
        })?,
    )?;

    // owlry.cache.has(key) -> boolean
    cache_table.set(
        "has",
        lua.create_function(|_lua, key: String| {
            let cache = CACHE.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock cache: {}", e))
            })?;

            if let Some(entry) = cache.get(&key) {
                Ok(!entry.is_expired())
            } else {
                Ok(false)
            }
        })?,
    )?;

    owlry.set("cache", cache_table)?;
    Ok(())
}

/// Convert Lua value to serde_json::Value
fn lua_value_to_json(value: &Value) -> LuaResult<serde_json::Value> {
    use serde_json::Value as JsonValue;

    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
        Value::Integer(i) => Ok(JsonValue::Number((*i).into())),
        Value::Number(n) => Ok(serde_json::Number::from_f64(*n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => lua_table_to_json(t),
        _ => Err(mlua::Error::external("Unsupported Lua type for cache")),
    }
}

/// Convert Lua table to serde_json::Value
fn lua_table_to_json(table: &Table) -> LuaResult<serde_json::Value> {
    use serde_json::{Map, Value as JsonValue};

    // Check if it's an array (sequential integer keys starting from 1)
    let is_array = table
        .clone()
        .pairs::<i64, Value>()
        .enumerate()
        .all(|(i, pair)| pair.map(|(k, _)| k == (i + 1) as i64).unwrap_or(false));

    if is_array {
        let mut arr = Vec::new();
        for pair in table.clone().pairs::<i64, Value>() {
            let (_, v) = pair?;
            arr.push(lua_value_to_json(&v)?);
        }
        Ok(JsonValue::Array(arr))
    } else {
        let mut map = Map::new();
        for pair in table.clone().pairs::<String, Value>() {
            let (k, v) = pair?;
            map.insert(k, lua_value_to_json(&v)?);
        }
        Ok(JsonValue::Object(map))
    }
}

/// Convert serde_json::Value to Lua value
fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    use serde_json::Value as JsonValue;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_cache_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        // Clear cache between tests
        CACHE.lock().unwrap().clear();

        lua
    }

    #[test]
    fn test_cache_set_get() {
        let lua = setup_lua();

        // Set a value
        let chunk = lua.load(r#"return owlry.cache.set("test_key", "test_value")"#);
        let result: bool = chunk.call(()).unwrap();
        assert!(result);

        // Get the value back
        let chunk = lua.load(r#"return owlry.cache.get("test_key")"#);
        let value: String = chunk.call(()).unwrap();
        assert_eq!(value, "test_value");
    }

    #[test]
    fn test_cache_table_value() {
        let lua = setup_lua();

        // Set a table value
        let chunk = lua.load(r#"return owlry.cache.set("table_key", {name = "test", value = 42})"#);
        let _: bool = chunk.call(()).unwrap();

        // Get and verify
        let chunk = lua.load(r#"
            local t = owlry.cache.get("table_key")
            return t.name, t.value
        "#);
        let (name, value): (String, i32) = chunk.call(()).unwrap();
        assert_eq!(name, "test");
        assert_eq!(value, 42);
    }

    #[test]
    fn test_cache_delete() {
        let lua = setup_lua();

        let chunk = lua.load(r#"
            owlry.cache.set("delete_key", "value")
            local existed = owlry.cache.delete("delete_key")
            local value = owlry.cache.get("delete_key")
            return existed, value
        "#);
        let (existed, value): (bool, Option<String>) = chunk.call(()).unwrap();
        assert!(existed);
        assert!(value.is_none());
    }

    #[test]
    fn test_cache_has() {
        let lua = setup_lua();

        let chunk = lua.load(r#"
            local before = owlry.cache.has("has_key")
            owlry.cache.set("has_key", "value")
            local after = owlry.cache.has("has_key")
            return before, after
        "#);
        let (before, after): (bool, bool) = chunk.call(()).unwrap();
        assert!(!before);
        assert!(after);
    }

    #[test]
    fn test_cache_missing_key() {
        let lua = setup_lua();

        let chunk = lua.load(r#"return owlry.cache.get("nonexistent_key")"#);
        let value: Value = chunk.call(()).unwrap();
        assert!(matches!(value, Value::Nil));
    }
}
