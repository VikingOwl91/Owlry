//! Lua API implementations for plugins
//!
//! This module provides the `owlry` global table and its submodules
//! that plugins can use to interact with owlry.

mod provider;
mod utils;

use mlua::{Lua, Result as LuaResult};
use owlry_plugin_api::PluginItem;

use crate::loader::ProviderRegistration;

/// Register all owlry APIs in the Lua runtime
pub fn register_apis(lua: &Lua, plugin_dir: &std::path::Path, plugin_id: &str) -> LuaResult<()> {
    let globals = lua.globals();

    // Create the main owlry table
    let owlry = lua.create_table()?;

    // Register utility APIs (log, path, fs, json)
    utils::register_log_api(lua, &owlry)?;
    utils::register_path_api(lua, &owlry, plugin_dir)?;
    utils::register_fs_api(lua, &owlry, plugin_dir)?;
    utils::register_json_api(lua, &owlry)?;

    // Register provider API
    provider::register_provider_api(lua, &owlry)?;

    // Set owlry as global
    globals.set("owlry", owlry)?;

    // Suppress unused warnings
    let _ = plugin_id;

    Ok(())
}

/// Get provider registrations from the Lua runtime
pub fn get_provider_registrations(lua: &Lua) -> LuaResult<Vec<ProviderRegistration>> {
    provider::get_registrations(lua)
}

/// Call a provider's refresh function
pub fn call_refresh(lua: &Lua, provider_name: &str) -> LuaResult<Vec<PluginItem>> {
    provider::call_refresh(lua, provider_name)
}

/// Call a provider's query function
pub fn call_query(lua: &Lua, provider_name: &str, query: &str) -> LuaResult<Vec<PluginItem>> {
    provider::call_query(lua, provider_name, query)
}
