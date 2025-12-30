//! Lua runtime setup and sandboxing

use mlua::{Lua, Result as LuaResult, StdLib};

use crate::manifest::PluginPermissions;

/// Configuration for the Lua sandbox
///
/// Note: Some fields are reserved for future sandbox enforcement.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SandboxConfig {
    /// Allow shell command running (reserved for future enforcement)
    pub allow_commands: bool,
    /// Allow HTTP requests (reserved for future enforcement)
    pub allow_network: bool,
    /// Allow filesystem access outside plugin directory (reserved for future enforcement)
    pub allow_external_fs: bool,
    /// Maximum run time per call (ms) (reserved for future enforcement)
    pub max_run_time_ms: u64,
    /// Memory limit (bytes, 0 = unlimited) (reserved for future enforcement)
    pub max_memory: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            allow_commands: false,
            allow_network: false,
            allow_external_fs: false,
            max_run_time_ms: 5000, // 5 seconds
            max_memory: 64 * 1024 * 1024, // 64 MB
        }
    }
}

impl SandboxConfig {
    /// Create a sandbox config from plugin permissions
    pub fn from_permissions(permissions: &PluginPermissions) -> Self {
        Self {
            allow_commands: !permissions.run_commands.is_empty(),
            allow_network: permissions.network,
            allow_external_fs: !permissions.filesystem.is_empty(),
            ..Default::default()
        }
    }
}

/// Create a new sandboxed Lua runtime
pub fn create_lua_runtime(_sandbox: &SandboxConfig) -> LuaResult<Lua> {
    // Create Lua with safe standard libraries only
    // We exclude: debug, io, os (dangerous parts), package (loadlib), ffi
    let libs = StdLib::COROUTINE
        | StdLib::TABLE
        | StdLib::STRING
        | StdLib::UTF8
        | StdLib::MATH;

    let lua = Lua::new_with(libs, mlua::LuaOptions::default())?;

    // Set up safe environment
    setup_safe_globals(&lua)?;

    Ok(lua)
}

/// Set up safe global environment by removing/replacing dangerous functions
fn setup_safe_globals(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // Remove dangerous globals
    globals.set("dofile", mlua::Value::Nil)?;
    globals.set("loadfile", mlua::Value::Nil)?;

    // Create a restricted os table with only safe functions
    let os_table = lua.create_table()?;
    os_table.set("clock", lua.create_function(|_, ()| {
        Ok(std::time::Instant::now().elapsed().as_secs_f64())
    })?)?;
    os_table.set("date", lua.create_function(os_date)?)?;
    os_table.set("difftime", lua.create_function(|_, (t2, t1): (f64, f64)| Ok(t2 - t1))?)?;
    os_table.set("time", lua.create_function(os_time)?)?;
    globals.set("os", os_table)?;

    // Remove print (plugins should use owlry.log instead)
    globals.set("print", mlua::Value::Nil)?;

    Ok(())
}

/// Safe os.date implementation
fn os_date(_lua: &Lua, format: Option<String>) -> LuaResult<String> {
    use chrono::Local;
    let now = Local::now();
    let fmt = format.unwrap_or_else(|| "%c".to_string());
    Ok(now.format(&fmt).to_string())
}

/// Safe os.time implementation
fn os_time(_lua: &Lua, _args: ()) -> LuaResult<i64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_secs() as i64)
}

/// Load and run a Lua file in the given runtime
pub fn load_file(lua: &Lua, path: &std::path::Path) -> LuaResult<()> {
    let content = std::fs::read_to_string(path)
        .map_err(mlua::Error::external)?;
    lua.load(&content)
        .set_name(path.file_name().and_then(|n| n.to_str()).unwrap_or("chunk"))
        .into_function()?
        .call(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sandboxed_runtime() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();

        // Verify dangerous functions are removed
        let result: LuaResult<mlua::Value> = lua.globals().get("dofile");
        assert!(matches!(result, Ok(mlua::Value::Nil)));

        // Verify safe functions work
        let result: String = lua.load("return os.date('%Y')").call(()).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_basic_lua_operations() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();

        // Test basic math
        let result: i32 = lua.load("return 2 + 2").call(()).unwrap();
        assert_eq!(result, 4);

        // Test table operations
        let result: i32 = lua.load("local t = {1,2,3}; return #t").call(()).unwrap();
        assert_eq!(result, 3);

        // Test string operations
        let result: String = lua.load("return string.upper('hello')").call(()).unwrap();
        assert_eq!(result, "HELLO");
    }
}
