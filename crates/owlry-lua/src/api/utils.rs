//! Utility APIs: logging, paths, filesystem, JSON

use mlua::{Lua, Result as LuaResult, Table, Value};
use std::path::{Path, PathBuf};

// ============================================================================
// Logging API
// ============================================================================

/// Register the log API in the owlry table
pub fn register_log_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let log = lua.create_table()?;

    log.set("debug", lua.create_function(|_, msg: String| {
        eprintln!("[DEBUG] {}", msg);
        Ok(())
    })?)?;

    log.set("info", lua.create_function(|_, msg: String| {
        eprintln!("[INFO] {}", msg);
        Ok(())
    })?)?;

    log.set("warn", lua.create_function(|_, msg: String| {
        eprintln!("[WARN] {}", msg);
        Ok(())
    })?)?;

    log.set("error", lua.create_function(|_, msg: String| {
        eprintln!("[ERROR] {}", msg);
        Ok(())
    })?)?;

    owlry.set("log", log)?;
    Ok(())
}

// ============================================================================
// Path API
// ============================================================================

/// Register the path API in the owlry table
pub fn register_path_api(lua: &Lua, owlry: &Table, plugin_dir: &Path) -> LuaResult<()> {
    let path = lua.create_table()?;

    // owlry.path.config() -> ~/.config/owlry
    path.set("config", lua.create_function(|_, ()| {
        Ok(dirs::config_dir()
            .map(|d| d.join("owlry"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default())
    })?)?;

    // owlry.path.data() -> ~/.local/share/owlry
    path.set("data", lua.create_function(|_, ()| {
        Ok(dirs::data_dir()
            .map(|d| d.join("owlry"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default())
    })?)?;

    // owlry.path.cache() -> ~/.cache/owlry
    path.set("cache", lua.create_function(|_, ()| {
        Ok(dirs::cache_dir()
            .map(|d| d.join("owlry"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default())
    })?)?;

    // owlry.path.home() -> ~
    path.set("home", lua.create_function(|_, ()| {
        Ok(dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default())
    })?)?;

    // owlry.path.join(...) -> joined path
    path.set("join", lua.create_function(|_, parts: mlua::Variadic<String>| {
        let mut path = PathBuf::new();
        for part in parts {
            path.push(part);
        }
        Ok(path.to_string_lossy().to_string())
    })?)?;

    // owlry.path.plugin_dir() -> plugin directory
    let plugin_dir_str = plugin_dir.to_string_lossy().to_string();
    path.set("plugin_dir", lua.create_function(move |_, ()| {
        Ok(plugin_dir_str.clone())
    })?)?;

    // owlry.path.expand(path) -> expanded path (~ -> home)
    path.set("expand", lua.create_function(|_, path: String| {
        if path.starts_with("~/")
            && let Some(home) = dirs::home_dir() {
                return Ok(home.join(&path[2..]).to_string_lossy().to_string());
            }
        Ok(path)
    })?)?;

    owlry.set("path", path)?;
    Ok(())
}

// ============================================================================
// Filesystem API
// ============================================================================

/// Register the fs API in the owlry table
pub fn register_fs_api(lua: &Lua, owlry: &Table, _plugin_dir: &Path) -> LuaResult<()> {
    let fs = lua.create_table()?;

    // owlry.fs.exists(path) -> bool
    fs.set("exists", lua.create_function(|_, path: String| {
        let path = expand_path(&path);
        Ok(Path::new(&path).exists())
    })?)?;

    // owlry.fs.is_dir(path) -> bool
    fs.set("is_dir", lua.create_function(|_, path: String| {
        let path = expand_path(&path);
        Ok(Path::new(&path).is_dir())
    })?)?;

    // owlry.fs.read(path) -> string or nil
    fs.set("read", lua.create_function(|_, path: String| {
        let path = expand_path(&path);
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(Some(content)),
            Err(_) => Ok(None),
        }
    })?)?;

    // owlry.fs.read_lines(path) -> table of strings or nil
    fs.set("read_lines", lua.create_function(|lua, path: String| {
        let path = expand_path(&path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                Ok(Some(lua.create_sequence_from(lines)?))
            }
            Err(_) => Ok(None),
        }
    })?)?;

    // owlry.fs.list_dir(path) -> table of filenames or nil
    fs.set("list_dir", lua.create_function(|lua, path: String| {
        let path = expand_path(&path);
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let names: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                Ok(Some(lua.create_sequence_from(names)?))
            }
            Err(_) => Ok(None),
        }
    })?)?;

    // owlry.fs.read_json(path) -> table or nil
    fs.set("read_json", lua.create_function(|lua, path: String| {
        let path = expand_path(&path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(value) => json_to_lua(lua, &value),
                    Err(_) => Ok(Value::Nil),
                }
            }
            Err(_) => Ok(Value::Nil),
        }
    })?)?;

    // owlry.fs.write(path, content) -> bool
    fs.set("write", lua.create_function(|_, (path, content): (String, String)| {
        let path = expand_path(&path);
        // Create parent directories if needed
        if let Some(parent) = Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Ok(std::fs::write(&path, content).is_ok())
    })?)?;

    owlry.set("fs", fs)?;
    Ok(())
}

// ============================================================================
// JSON API
// ============================================================================

/// Register the json API in the owlry table
pub fn register_json_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let json = lua.create_table()?;

    // owlry.json.encode(value) -> string
    json.set("encode", lua.create_function(|lua, value: Value| {
        let json_value = lua_to_json(lua, &value)?;
        Ok(serde_json::to_string(&json_value).unwrap_or_else(|_| "null".to_string()))
    })?)?;

    // owlry.json.decode(string) -> value or nil
    json.set("decode", lua.create_function(|lua, s: String| {
        match serde_json::from_str::<serde_json::Value>(&s) {
            Ok(value) => json_to_lua(lua, &value),
            Err(_) => Ok(Value::Nil),
        }
    })?)?;

    owlry.set("json", json)?;
    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Expand ~ in paths
fn expand_path(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]).to_string_lossy().to_string();
        }
    path.to_string()
}

/// Convert JSON value to Lua value
fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Convert Lua value to JSON value
fn lua_to_json(_lua: &Lua, value: &Value) -> LuaResult<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Number(n) => Ok(serde_json::json!(*n)),
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting from 1)
            let mut is_array = true;
            let mut max_key = 0i64;
            for pair in t.clone().pairs::<Value, Value>() {
                let (k, _) = pair?;
                match k {
                    Value::Integer(i) if i > 0 => {
                        max_key = max_key.max(i);
                    }
                    _ => {
                        is_array = false;
                        break;
                    }
                }
            }

            if is_array && max_key > 0 {
                let mut arr = Vec::new();
                for i in 1..=max_key {
                    let v: Value = t.get(i)?;
                    arr.push(lua_to_json(_lua, &v)?);
                }
                Ok(serde_json::Value::Array(arr))
            } else {
                let mut obj = serde_json::Map::new();
                for pair in t.clone().pairs::<String, Value>() {
                    let (k, v) = pair?;
                    obj.insert(k, lua_to_json(_lua, &v)?);
                }
                Ok(serde_json::Value::Object(obj))
            }
        }
        _ => Ok(serde_json::Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{create_lua_runtime, SandboxConfig};

    #[test]
    fn test_log_api() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();
        let owlry = lua.create_table().unwrap();
        register_log_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        // Just verify it doesn't panic
        lua.load("owlry.log.info('test message')").set_name("test").call::<()>(()).unwrap();
    }

    #[test]
    fn test_path_api() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();
        let owlry = lua.create_table().unwrap();
        register_path_api(&lua, &owlry, Path::new("/tmp/test-plugin")).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        let home: String = lua.load("return owlry.path.home()").set_name("test").call(()).unwrap();
        assert!(!home.is_empty());

        let plugin_dir: String = lua.load("return owlry.path.plugin_dir()").set_name("test").call(()).unwrap();
        assert_eq!(plugin_dir, "/tmp/test-plugin");
    }

    #[test]
    fn test_fs_api() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();
        let owlry = lua.create_table().unwrap();
        register_fs_api(&lua, &owlry, Path::new("/tmp")).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        let exists: bool = lua.load("return owlry.fs.exists('/tmp')").set_name("test").call(()).unwrap();
        assert!(exists);

        let is_dir: bool = lua.load("return owlry.fs.is_dir('/tmp')").set_name("test").call(()).unwrap();
        assert!(is_dir);
    }

    #[test]
    fn test_json_api() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();
        let owlry = lua.create_table().unwrap();
        register_json_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        let code = r#"
            local t = { name = "test", value = 42 }
            local json = owlry.json.encode(t)
            local decoded = owlry.json.decode(json)
            return decoded.name, decoded.value
        "#;
        let (name, value): (String, i32) = lua.load(code).set_name("test").call(()).unwrap();
        assert_eq!(name, "test");
        assert_eq!(value, 42);
    }
}
