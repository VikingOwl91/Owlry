//! `owlry.util.*` host helpers exposed to user Lua configs.
//!
//! Surface per `docs/lua-api.md` §7:
//!
//! - `owlry.util.shell(cmd) -> string`        — capture stdout (trimmed)
//! - `owlry.util.shell_lines(cmd) -> {string}`
//! - `owlry.util.read_file(path) -> string`
//! - `owlry.util.glob(pattern) -> {string}`
//! - `owlry.util.env(name) -> string|nil`
//! - `owlry.util.hostname() -> string`
//!
//! Phase 3.1: stub module. Implementations land in 3.6.

use mlua::Lua;

use super::error::LuaConfigError;

#[allow(dead_code)]
pub(crate) fn install(_lua: &Lua) -> Result<(), LuaConfigError> {
    // TODO(phase-3.6): build owlry.util table with shell / shell_lines /
    //                  read_file / glob / env / hostname helpers.
    Ok(())
}
