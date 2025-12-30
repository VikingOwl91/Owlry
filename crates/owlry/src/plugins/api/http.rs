//! HTTP client API for Lua plugins
//!
//! Provides:
//! - `owlry.http.get(url, opts)` - HTTP GET request
//! - `owlry.http.post(url, body, opts)` - HTTP POST request

use mlua::{Lua, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Register HTTP client APIs
pub fn register_http_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let http_table = lua.create_table()?;

    // owlry.http.get(url, opts?) -> { status, body, headers }
    http_table.set(
        "get",
        lua.create_function(|lua, (url, opts): (String, Option<Table>)| {
            log::debug!("[plugin] http.get: {}", url);

            let timeout_secs = opts
                .as_ref()
                .and_then(|o| o.get::<u64>("timeout").ok())
                .unwrap_or(30);

            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .map_err(|e| mlua::Error::external(format!("Failed to create HTTP client: {}", e)))?;

            let mut request = client.get(&url);

            // Add custom headers if provided
            if let Some(ref opts) = opts
                && let Ok(headers) = opts.get::<Table>("headers") {
                    for pair in headers.pairs::<String, String>() {
                        let (key, value) = pair?;
                        request = request.header(&key, &value);
                    }
                }

            let response = request
                .send()
                .map_err(|e| mlua::Error::external(format!("HTTP request failed: {}", e)))?;

            let status = response.status().as_u16();
            let headers = extract_headers(&response);
            let body = response
                .text()
                .map_err(|e| mlua::Error::external(format!("Failed to read response body: {}", e)))?;

            let result = lua.create_table()?;
            result.set("status", status)?;
            result.set("body", body)?;
            result.set("ok", (200..300).contains(&status))?;

            let headers_table = lua.create_table()?;
            for (key, value) in headers {
                headers_table.set(key, value)?;
            }
            result.set("headers", headers_table)?;

            Ok(result)
        })?,
    )?;

    // owlry.http.post(url, body, opts?) -> { status, body, headers }
    http_table.set(
        "post",
        lua.create_function(|lua, (url, body, opts): (String, Value, Option<Table>)| {
            log::debug!("[plugin] http.post: {}", url);

            let timeout_secs = opts
                .as_ref()
                .and_then(|o| o.get::<u64>("timeout").ok())
                .unwrap_or(30);

            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .map_err(|e| mlua::Error::external(format!("Failed to create HTTP client: {}", e)))?;

            let mut request = client.post(&url);

            // Add custom headers if provided
            if let Some(ref opts) = opts
                && let Ok(headers) = opts.get::<Table>("headers") {
                    for pair in headers.pairs::<String, String>() {
                        let (key, value) = pair?;
                        request = request.header(&key, &value);
                    }
                }

            // Set body based on type
            request = match body {
                Value::String(s) => request.body(s.to_str()?.to_string()),
                Value::Table(t) => {
                    // Assume JSON if body is a table
                    let json_str = table_to_json(&t)?;
                    request
                        .header("Content-Type", "application/json")
                        .body(json_str)
                }
                Value::Nil => request,
                _ => {
                    return Err(mlua::Error::external(
                        "POST body must be a string or table",
                    ))
                }
            };

            let response = request
                .send()
                .map_err(|e| mlua::Error::external(format!("HTTP request failed: {}", e)))?;

            let status = response.status().as_u16();
            let headers = extract_headers(&response);
            let body = response
                .text()
                .map_err(|e| mlua::Error::external(format!("Failed to read response body: {}", e)))?;

            let result = lua.create_table()?;
            result.set("status", status)?;
            result.set("body", body)?;
            result.set("ok", (200..300).contains(&status))?;

            let headers_table = lua.create_table()?;
            for (key, value) in headers {
                headers_table.set(key, value)?;
            }
            result.set("headers", headers_table)?;

            Ok(result)
        })?,
    )?;

    // owlry.http.get_json(url, opts?) -> parsed JSON as table
    // Convenience function that parses JSON response
    http_table.set(
        "get_json",
        lua.create_function(|lua, (url, opts): (String, Option<Table>)| {
            log::debug!("[plugin] http.get_json: {}", url);

            let timeout_secs = opts
                .as_ref()
                .and_then(|o| o.get::<u64>("timeout").ok())
                .unwrap_or(30);

            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .map_err(|e| mlua::Error::external(format!("Failed to create HTTP client: {}", e)))?;

            let mut request = client.get(&url);
            request = request.header("Accept", "application/json");

            // Add custom headers if provided
            if let Some(ref opts) = opts
                && let Ok(headers) = opts.get::<Table>("headers") {
                    for pair in headers.pairs::<String, String>() {
                        let (key, value) = pair?;
                        request = request.header(&key, &value);
                    }
                }

            let response = request
                .send()
                .map_err(|e| mlua::Error::external(format!("HTTP request failed: {}", e)))?;

            if !response.status().is_success() {
                return Err(mlua::Error::external(format!(
                    "HTTP request failed with status {}",
                    response.status()
                )));
            }

            let body = response
                .text()
                .map_err(|e| mlua::Error::external(format!("Failed to read response body: {}", e)))?;

            // Parse JSON and convert to Lua table
            let json_value: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| mlua::Error::external(format!("Failed to parse JSON: {}", e)))?;

            json_to_lua(lua, &json_value)
        })?,
    )?;

    owlry.set("http", http_table)?;
    Ok(())
}

/// Extract headers from response into a HashMap
fn extract_headers(response: &reqwest::blocking::Response) -> HashMap<String, String> {
    response
        .headers()
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|v| (k.as_str().to_lowercase(), v.to_string()))
        })
        .collect()
}

/// Convert a Lua table to JSON string
fn table_to_json(table: &Table) -> LuaResult<String> {
    let value = lua_to_json(table)?;
    serde_json::to_string(&value)
        .map_err(|e| mlua::Error::external(format!("Failed to serialize to JSON: {}", e)))
}

/// Convert Lua table to serde_json::Value
fn lua_to_json(table: &Table) -> LuaResult<serde_json::Value> {
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

/// Convert a single Lua value to JSON
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
        Value::Table(t) => lua_to_json(t),
        _ => Err(mlua::Error::external("Unsupported Lua type for JSON")),
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
        register_http_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_json_conversion() {
        let lua = setup_lua();

        // Test table to JSON
        let table = lua.create_table().unwrap();
        table.set("name", "test").unwrap();
        table.set("value", 42).unwrap();

        let json = table_to_json(&table).unwrap();
        assert!(json.contains("name"));
        assert!(json.contains("test"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_array_to_json() {
        let lua = setup_lua();

        let table = lua.create_table().unwrap();
        table.set(1, "first").unwrap();
        table.set(2, "second").unwrap();
        table.set(3, "third").unwrap();

        let json = table_to_json(&table).unwrap();
        assert!(json.starts_with('['));
        assert!(json.contains("first"));
    }

    // Note: Network tests are skipped in CI - they require internet access
    // Use `cargo test -- --ignored` to run them locally
    #[test]
    #[ignore]
    fn test_http_get() {
        let lua = setup_lua();
        let chunk = lua.load(r#"return owlry.http.get("https://httpbin.org/get")"#);
        let result: Table = chunk.call(()).unwrap();

        assert_eq!(result.get::<u16>("status").unwrap(), 200);
        assert!(result.get::<bool>("ok").unwrap());
    }
}
