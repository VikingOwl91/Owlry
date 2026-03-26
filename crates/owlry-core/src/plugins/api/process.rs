//! Process and environment APIs for Lua plugins
//!
//! Provides:
//! - `owlry.process.run(cmd)` - Run a shell command and return output
//! - `owlry.process.exists(cmd)` - Check if a command exists in PATH
//! - `owlry.env.get(name)` - Get an environment variable
//! - `owlry.env.set(name, value)` - Set an environment variable (for plugin scope)

use mlua::{Lua, Result as LuaResult, Table};
use std::process::Command;

/// Register process-related APIs
pub fn register_process_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let process_table = lua.create_table()?;

    // owlry.process.run(cmd) -> { stdout, stderr, exit_code, success }
    // Runs a shell command and returns the result
    process_table.set(
        "run",
        lua.create_function(|lua, cmd: String| {
            log::debug!("[plugin] process.run: {}", cmd);

            let output = Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .map_err(|e| mlua::Error::external(format!("Failed to run command: {}", e)))?;

            let result = lua.create_table()?;
            result.set("stdout", String::from_utf8_lossy(&output.stdout).to_string())?;
            result.set("stderr", String::from_utf8_lossy(&output.stderr).to_string())?;
            result.set("exit_code", output.status.code().unwrap_or(-1))?;
            result.set("success", output.status.success())?;

            Ok(result)
        })?,
    )?;

    // owlry.process.run_lines(cmd) -> table of lines
    // Convenience function that runs a command and returns stdout split into lines
    process_table.set(
        "run_lines",
        lua.create_function(|lua, cmd: String| {
            log::debug!("[plugin] process.run_lines: {}", cmd);

            let output = Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .map_err(|e| mlua::Error::external(format!("Failed to run command: {}", e)))?;

            if !output.status.success() {
                return Err(mlua::Error::external(format!(
                    "Command failed with exit code {}: {}",
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().collect();

            let result = lua.create_table()?;
            for (i, line) in lines.iter().enumerate() {
                result.set(i + 1, *line)?;
            }

            Ok(result)
        })?,
    )?;

    // owlry.process.exists(cmd) -> boolean
    // Checks if a command exists in PATH
    process_table.set(
        "exists",
        lua.create_function(|_lua, cmd: String| {
            let exists = Command::new("which")
                .arg(&cmd)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            Ok(exists)
        })?,
    )?;

    owlry.set("process", process_table)?;
    Ok(())
}

/// Register environment variable APIs
pub fn register_env_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let env_table = lua.create_table()?;

    // owlry.env.get(name) -> string or nil
    env_table.set(
        "get",
        lua.create_function(|_lua, name: String| {
            Ok(std::env::var(&name).ok())
        })?,
    )?;

    // owlry.env.get_or(name, default) -> string
    env_table.set(
        "get_or",
        lua.create_function(|_lua, (name, default): (String, String)| {
            Ok(std::env::var(&name).unwrap_or(default))
        })?,
    )?;

    // owlry.env.home() -> string
    // Convenience function to get home directory
    env_table.set(
        "home",
        lua.create_function(|_lua, ()| {
            Ok(dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
        })?,
    )?;

    owlry.set("env", env_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_process_api(&lua, &owlry).unwrap();
        register_env_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_process_run() {
        let lua = setup_lua();
        let chunk = lua.load(r#"return owlry.process.run("echo hello")"#);
        let result: Table = chunk.call(()).unwrap();

        assert_eq!(result.get::<bool>("success").unwrap(), true);
        assert_eq!(result.get::<i32>("exit_code").unwrap(), 0);
        assert!(result.get::<String>("stdout").unwrap().contains("hello"));
    }

    #[test]
    fn test_process_run_lines() {
        let lua = setup_lua();
        let chunk = lua.load(r#"return owlry.process.run_lines("echo -e 'line1\nline2\nline3'")"#);
        let result: Table = chunk.call(()).unwrap();

        assert_eq!(result.get::<String>(1).unwrap(), "line1");
        assert_eq!(result.get::<String>(2).unwrap(), "line2");
        assert_eq!(result.get::<String>(3).unwrap(), "line3");
    }

    #[test]
    fn test_process_exists() {
        let lua = setup_lua();

        // 'sh' should always exist
        let chunk = lua.load(r#"return owlry.process.exists("sh")"#);
        let exists: bool = chunk.call(()).unwrap();
        assert!(exists);

        // Made-up command should not exist
        let chunk = lua.load(r#"return owlry.process.exists("this_command_definitely_does_not_exist_12345")"#);
        let not_exists: bool = chunk.call(()).unwrap();
        assert!(!not_exists);
    }

    #[test]
    fn test_env_get() {
        let lua = setup_lua();

        // HOME should be set on any Unix system
        let chunk = lua.load(r#"return owlry.env.get("HOME")"#);
        let home: Option<String> = chunk.call(()).unwrap();
        assert!(home.is_some());

        // Non-existent variable should return nil
        let chunk = lua.load(r#"return owlry.env.get("THIS_VAR_DOES_NOT_EXIST_12345")"#);
        let missing: Option<String> = chunk.call(()).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_env_get_or() {
        let lua = setup_lua();

        let chunk = lua.load(r#"return owlry.env.get_or("THIS_VAR_DOES_NOT_EXIST_12345", "default_value")"#);
        let result: String = chunk.call(()).unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_env_home() {
        let lua = setup_lua();

        let chunk = lua.load(r#"return owlry.env.home()"#);
        let home: Option<String> = chunk.call(()).unwrap();
        assert!(home.is_some());
        assert!(home.unwrap().starts_with('/'));
    }
}
