//! Lua runtime wrapper.
//!
//! [`LuaContext`] owns a single `mlua::Lua` state and the `owlry.*` API
//! surface installed on it. Phase 3.1 only stubs the type — actual table
//! installation and evaluation land in 3.2+.

use std::path::Path;

use mlua::Lua;

use super::error::LuaConfigError;

#[allow(dead_code)]
pub struct LuaContext {
    lua: Lua,
}

#[allow(dead_code)]
impl LuaContext {
    /// Build a fresh Lua state with the `owlry.*` API surface installed.
    ///
    /// Stub: returns a bare `Lua` state. Phase 3.2 wires `owlry.set`,
    /// `owlry.providers`, `owlry.tabs`; later sub-phases add `provider`,
    /// `theme`, `profiles`, and `util`.
    pub fn new() -> Result<Self, LuaConfigError> {
        let lua = Lua::new();
        Ok(Self { lua })
    }

    /// Evaluate the user's `owlry.lua` file against this context.
    ///
    /// Stub: not wired in 3.1. Phase 3.4 hooks this into the config
    /// resolution path (`owlry.lua` > `config.toml` > defaults).
    pub fn eval_file(&self, _path: &Path) -> Result<(), LuaConfigError> {
        // TODO(phase-3.2): load file via std::fs, evaluate with self.lua,
        //                  capture and convert mlua::Error into Eval { path, .. }.
        Ok(())
    }

    /// Expose the underlying Lua state for sub-phases that need to register
    /// host functions or read collected state back out.
    pub(crate) fn raw(&self) -> &Lua {
        &self.lua
    }
}
