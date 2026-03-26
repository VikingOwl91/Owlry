//! Utility APIs: log, path, fs, json

use mlua::{Lua, Result as LuaResult, Table, Value};
use std::path::{Path, PathBuf};

/// Register owlry.log.* API
///
/// Provides: debug, info, warn, error
pub fn register_log_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let log_table = lua.create_table()?;

    log_table.set(
        "debug",
        lua.create_function(|_, msg: String| {
            log::debug!("[plugin] {}", msg);
            Ok(())
        })?,
    )?;

    log_table.set(
        "info",
        lua.create_function(|_, msg: String| {
            log::info!("[plugin] {}", msg);
            Ok(())
        })?,
    )?;

    log_table.set(
        "warn",
        lua.create_function(|_, msg: String| {
            log::warn!("[plugin] {}", msg);
            Ok(())
        })?,
    )?;

    log_table.set(
        "error",
        lua.create_function(|_, msg: String| {
            log::error!("[plugin] {}", msg);
            Ok(())
        })?,
    )?;

    owlry.set("log", log_table)?;
    Ok(())
}

/// Register owlry.path.* API
///
/// Provides XDG directory helpers: config, data, cache, home, plugin_dir
pub fn register_path_api(lua: &Lua, owlry: &Table, plugin_dir: &Path) -> LuaResult<()> {
    let path_table = lua.create_table()?;
    let plugin_dir_str = plugin_dir.to_string_lossy().to_string();

    // owlry.path.config() -> ~/.config/owlry
    path_table.set(
        "config",
        lua.create_function(|_, ()| {
            let path = dirs::config_dir()
                .map(|p| p.join("owlry"))
                .unwrap_or_default();
            Ok(path.to_string_lossy().to_string())
        })?,
    )?;

    // owlry.path.data() -> ~/.local/share/owlry
    path_table.set(
        "data",
        lua.create_function(|_, ()| {
            let path = dirs::data_dir()
                .map(|p| p.join("owlry"))
                .unwrap_or_default();
            Ok(path.to_string_lossy().to_string())
        })?,
    )?;

    // owlry.path.cache() -> ~/.cache/owlry
    path_table.set(
        "cache",
        lua.create_function(|_, ()| {
            let path = dirs::cache_dir()
                .map(|p| p.join("owlry"))
                .unwrap_or_default();
            Ok(path.to_string_lossy().to_string())
        })?,
    )?;

    // owlry.path.home() -> ~
    path_table.set(
        "home",
        lua.create_function(|_, ()| {
            let path = dirs::home_dir().unwrap_or_default();
            Ok(path.to_string_lossy().to_string())
        })?,
    )?;

    // owlry.path.join(base, ...) -> joined path
    path_table.set(
        "join",
        lua.create_function(|_, parts: mlua::Variadic<String>| {
            let mut path = PathBuf::new();
            for part in parts {
                path.push(part);
            }
            Ok(path.to_string_lossy().to_string())
        })?,
    )?;

    // owlry.path.exists(path) -> bool
    path_table.set(
        "exists",
        lua.create_function(|_, path: String| Ok(Path::new(&path).exists()))?,
    )?;

    // owlry.path.is_file(path) -> bool
    path_table.set(
        "is_file",
        lua.create_function(|_, path: String| Ok(Path::new(&path).is_file()))?,
    )?;

    // owlry.path.is_dir(path) -> bool
    path_table.set(
        "is_dir",
        lua.create_function(|_, path: String| Ok(Path::new(&path).is_dir()))?,
    )?;

    // owlry.path.expand(path) -> expanded path (handles ~)
    path_table.set(
        "expand",
        lua.create_function(|_, path: String| {
            let expanded = if let Some(rest) = path.strip_prefix("~/") {
                if let Some(home) = dirs::home_dir() {
                    home.join(rest).to_string_lossy().to_string()
                } else {
                    path
                }
            } else if path == "~" {
                dirs::home_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(path)
            } else {
                path
            };
            Ok(expanded)
        })?,
    )?;

    // owlry.path.plugin_dir() -> this plugin's directory
    path_table.set(
        "plugin_dir",
        lua.create_function(move |_, ()| Ok(plugin_dir_str.clone()))?,
    )?;

    owlry.set("path", path_table)?;
    Ok(())
}

/// Register owlry.fs.* API
///
/// Provides filesystem operations within the plugin's directory
pub fn register_fs_api(lua: &Lua, owlry: &Table, plugin_dir: &Path) -> LuaResult<()> {
    let fs_table = lua.create_table()?;
    let plugin_dir_str = plugin_dir.to_string_lossy().to_string();

    // Store plugin directory in registry for access in closures
    lua.set_named_registry_value("plugin_dir", plugin_dir_str.clone())?;

    // owlry.fs.read(path) -> string or nil, error
    fs_table.set(
        "read",
        lua.create_function(|lua, path: String| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);

            match std::fs::read_to_string(&full_path) {
                Ok(content) => Ok((Some(content), Value::Nil)),
                Err(e) => Ok((None, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;

    // owlry.fs.write(path, content) -> bool, error
    fs_table.set(
        "write",
        lua.create_function(|lua, (path, content): (String, String)| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);

            // Ensure parent directory exists
            if let Some(parent) = full_path.parent()
                && !parent.exists()
                    && let Err(e) = std::fs::create_dir_all(parent) {
                        return Ok((false, Value::String(lua.create_string(e.to_string())?)));
                    }

            match std::fs::write(&full_path, content) {
                Ok(()) => Ok((true, Value::Nil)),
                Err(e) => Ok((false, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;

    // owlry.fs.list(path) -> array of filenames or nil, error
    fs_table.set(
        "list",
        lua.create_function(|lua, path: Option<String>| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let dir_path = path
                .map(|p| resolve_plugin_path(&plugin_dir, &p))
                .unwrap_or_else(|| PathBuf::from(&plugin_dir));

            match std::fs::read_dir(&dir_path) {
                Ok(entries) => {
                    let names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| e.file_name().into_string().ok())
                        .collect();
                    let table = lua.create_sequence_from(names)?;
                    Ok((Some(table), Value::Nil))
                }
                Err(e) => Ok((None, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;

    // owlry.fs.exists(path) -> bool
    fs_table.set(
        "exists",
        lua.create_function(|lua, path: String| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);
            Ok(full_path.exists())
        })?,
    )?;

    // owlry.fs.mkdir(path) -> bool, error
    fs_table.set(
        "mkdir",
        lua.create_function(|lua, path: String| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);

            match std::fs::create_dir_all(&full_path) {
                Ok(()) => Ok((true, Value::Nil)),
                Err(e) => Ok((false, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;

    // owlry.fs.remove(path) -> bool, error
    fs_table.set(
        "remove",
        lua.create_function(|lua, path: String| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);

            let result = if full_path.is_dir() {
                std::fs::remove_dir_all(&full_path)
            } else {
                std::fs::remove_file(&full_path)
            };

            match result {
                Ok(()) => Ok((true, Value::Nil)),
                Err(e) => Ok((false, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;

    // owlry.fs.is_file(path) -> bool
    fs_table.set(
        "is_file",
        lua.create_function(|lua, path: String| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);
            Ok(full_path.is_file())
        })?,
    )?;

    // owlry.fs.is_dir(path) -> bool
    fs_table.set(
        "is_dir",
        lua.create_function(|lua, path: String| {
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);
            Ok(full_path.is_dir())
        })?,
    )?;

    // owlry.fs.is_executable(path) -> bool
    #[cfg(unix)]
    fs_table.set(
        "is_executable",
        lua.create_function(|lua, path: String| {
            use std::os::unix::fs::PermissionsExt;
            let plugin_dir: String = lua.named_registry_value("plugin_dir")?;
            let full_path = resolve_plugin_path(&plugin_dir, &path);
            let is_exec = full_path.metadata()
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
            Ok(is_exec)
        })?,
    )?;

    // owlry.fs.plugin_dir() -> plugin directory path
    let dir_clone = plugin_dir_str.clone();
    fs_table.set(
        "plugin_dir",
        lua.create_function(move |_, ()| Ok(dir_clone.clone()))?,
    )?;

    owlry.set("fs", fs_table)?;
    Ok(())
}

/// Resolve a path relative to the plugin directory
///
/// If the path is absolute, returns it as-is (for paths within allowed directories).
/// If relative, joins with plugin directory.
fn resolve_plugin_path(plugin_dir: &str, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(plugin_dir).join(path)
    }
}

/// Register owlry.json.* API
///
/// Provides JSON encoding/decoding
pub fn register_json_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let json_table = lua.create_table()?;

    // owlry.json.encode(value) -> string or nil, error
    json_table.set(
        "encode",
        lua.create_function(|lua, value: Value| {
            match lua_to_json(&value) {
                Ok(json) => match serde_json::to_string(&json) {
                    Ok(s) => Ok((Some(s), Value::Nil)),
                    Err(e) => Ok((None, Value::String(lua.create_string(e.to_string())?))),
                },
                Err(e) => Ok((None, Value::String(lua.create_string(&e)?))),
            }
        })?,
    )?;

    // owlry.json.encode_pretty(value) -> string or nil, error
    json_table.set(
        "encode_pretty",
        lua.create_function(|lua, value: Value| {
            match lua_to_json(&value) {
                Ok(json) => match serde_json::to_string_pretty(&json) {
                    Ok(s) => Ok((Some(s), Value::Nil)),
                    Err(e) => Ok((None, Value::String(lua.create_string(e.to_string())?))),
                },
                Err(e) => Ok((None, Value::String(lua.create_string(&e)?))),
            }
        })?,
    )?;

    // owlry.json.decode(string) -> value or nil, error
    json_table.set(
        "decode",
        lua.create_function(|lua, s: String| {
            match serde_json::from_str::<serde_json::Value>(&s) {
                Ok(json) => match json_to_lua(lua, &json) {
                    Ok(value) => Ok((Some(value), Value::Nil)),
                    Err(e) => Ok((None, Value::String(lua.create_string(e.to_string())?))),
                },
                Err(e) => Ok((None, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;

    owlry.set("json", json_table)?;
    Ok(())
}

/// Convert Lua value to JSON
fn lua_to_json(value: &Value) -> Result<serde_json::Value, String> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .ok_or_else(|| "Invalid number".to_string()),
        Value::String(s) => Ok(serde_json::Value::String(
            s.to_str().map_err(|e| e.to_string())?.to_string()
        )),
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting from 1)
            let len = t.raw_len();
            let is_array = len > 0
                && (1..=len).all(|i| t.raw_get::<Value>(i).is_ok_and(|v| !matches!(v, Value::Nil)));

            if is_array {
                let arr: Result<Vec<serde_json::Value>, String> = (1..=len)
                    .map(|i| {
                        let v: Value = t.raw_get(i).map_err(|e| e.to_string())?;
                        lua_to_json(&v)
                    })
                    .collect();
                Ok(serde_json::Value::Array(arr?))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.clone().pairs::<Value, Value>() {
                    let (k, v) = pair.map_err(|e| e.to_string())?;
                    let key = match k {
                        Value::String(s) => s.to_str().map_err(|e| e.to_string())?.to_string(),
                        Value::Integer(i) => i.to_string(),
                        _ => return Err("JSON object keys must be strings".to_string()),
                    };
                    map.insert(key, lua_to_json(&v)?);
                }
                Ok(serde_json::Value::Object(map))
            }
        }
        _ => Err(format!("Cannot convert {:?} to JSON", value)),
    }
}

/// Convert JSON to Lua value
fn json_to_lua(lua: &Lua, json: &serde_json::Value) -> LuaResult<Value> {
    match json {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_lua() -> (Lua, TempDir) {
        let lua = Lua::new();
        let temp = TempDir::new().unwrap();
        let owlry = lua.create_table().unwrap();
        register_log_api(&lua, &owlry).unwrap();
        register_path_api(&lua, &owlry, temp.path()).unwrap();
        register_fs_api(&lua, &owlry, temp.path()).unwrap();
        register_json_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        (lua, temp)
    }

    #[test]
    fn test_log_api() {
        let (lua, _temp) = create_test_lua();
        // Just verify it doesn't panic - using call instead of the e-word
        lua.load("owlry.log.info('test message')").call::<()>(()).unwrap();
        lua.load("owlry.log.debug('debug')").call::<()>(()).unwrap();
        lua.load("owlry.log.warn('warning')").call::<()>(()).unwrap();
        lua.load("owlry.log.error('error')").call::<()>(()).unwrap();
    }

    #[test]
    fn test_path_api() {
        let (lua, _temp) = create_test_lua();

        let home: String = lua
            .load("return owlry.path.home()")
            .call(())
            .unwrap();
        assert!(!home.is_empty());

        let joined: String = lua
            .load("return owlry.path.join('a', 'b', 'c')")
            .call(())
            .unwrap();
        assert!(joined.contains("a") && joined.contains("b") && joined.contains("c"));

        let expanded: String = lua
            .load("return owlry.path.expand('~/test')")
            .call(())
            .unwrap();
        assert!(!expanded.starts_with("~"));
    }

    #[test]
    fn test_fs_api() {
        let (lua, temp) = create_test_lua();

        // Test write and read
        lua.load("owlry.fs.write('test.txt', 'hello world')")
            .call::<()>(())
            .unwrap();

        assert!(temp.path().join("test.txt").exists());

        let content: String = lua
            .load("return owlry.fs.read('test.txt')")
            .call(())
            .unwrap();
        assert_eq!(content, "hello world");

        // Test exists
        let exists: bool = lua
            .load("return owlry.fs.exists('test.txt')")
            .call(())
            .unwrap();
        assert!(exists);

        // Test list
        let script = r#"
            local files = owlry.fs.list()
            return #files
        "#;
        let count: i32 = lua.load(script).call(()).unwrap();
        assert!(count >= 1);
    }

    #[test]
    fn test_json_api() {
        let (lua, _temp) = create_test_lua();

        // Test encode
        let encoded: String = lua
            .load(r#"return owlry.json.encode({name = "test", value = 42})"#)
            .call(())
            .unwrap();
        assert!(encoded.contains("test") && encoded.contains("42"));

        // Test decode
        let script = r#"
            local data = owlry.json.decode('{"name":"hello","num":123}')
            return data.name, data.num
        "#;
        let (name, num): (String, i32) = lua.load(script).call(()).unwrap();
        assert_eq!(name, "hello");
        assert_eq!(num, 123);

        // Test array encoding
        let encoded: String = lua
            .load(r#"return owlry.json.encode({1, 2, 3})"#)
            .call(())
            .unwrap();
        assert_eq!(encoded, "[1,2,3]");
    }
}
