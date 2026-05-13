//! `owlry.*` Lua API surface.
//!
//! Installs the top-level `owlry` table and its members on the Lua state.
//! Member surface per `docs/lua-api.md` §4–§7:
//!
//! - `owlry.set(table)`         — scalar overrides (Phase 3.2)
//! - `owlry.providers(table)`   — per-provider enable / config (Phase 3.2)
//! - `owlry.tabs(list)`         — tab order + visibility (Phase 3.2)
//! - `owlry.provider(table)`    — register a Lua-defined provider (Phase 3.3)
//! - `owlry.theme(table|string)`— theme override (Phase 3.5)
//! - `owlry.profiles(table)`    — named profiles (Phase 3.5)
//! - `owlry.util.*`             — host helpers (Phase 3.6)
//!
//! Phase 3.1: stub registration entry point only. No tables created yet.

use mlua::Lua;

use super::error::LuaConfigError;

#[allow(dead_code)]
pub(crate) fn install(_lua: &Lua) -> Result<(), LuaConfigError> {
    // TODO(phase-3.2): create owlry table, register set / providers / tabs.
    // TODO(phase-3.3): register owlry.provider().
    // TODO(phase-3.5): register owlry.theme(), owlry.profiles().
    // TODO(phase-3.6): register owlry.util sub-table.
    Ok(())
}
