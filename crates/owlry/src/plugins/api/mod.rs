//! Lua API implementations for plugins
//!
//! This module provides the `owlry` global table and its submodules
//! that plugins can use to interact with owlry.

pub mod action;
mod cache;
pub mod hook;
mod http;
mod math;
mod process;
pub mod provider;
pub mod theme;
mod utils;

use mlua::{Lua, Result as LuaResult};

pub use action::ActionRegistration;
pub use hook::HookEvent;
pub use provider::ProviderRegistration;
pub use theme::ThemeRegistration;

/// Register all owlry APIs in the Lua runtime
///
/// This creates the `owlry` global table with all available APIs:
/// - `owlry.log.*` - Logging functions
/// - `owlry.path.*` - XDG path helpers
/// - `owlry.fs.*` - Filesystem operations
/// - `owlry.json.*` - JSON encode/decode
/// - `owlry.provider.*` - Provider registration
/// - `owlry.process.*` - Process execution
/// - `owlry.env.*` - Environment variables
/// - `owlry.http.*` - HTTP client
/// - `owlry.cache.*` - In-memory caching
/// - `owlry.math.*` - Math expression evaluation
/// - `owlry.hook.*` - Event hooks
/// - `owlry.action.*` - Custom actions
/// - `owlry.theme.*` - Theme registration
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

    // Register extended APIs (Phase 3)
    process::register_process_api(lua, &owlry)?;
    process::register_env_api(lua, &owlry)?;
    http::register_http_api(lua, &owlry)?;
    cache::register_cache_api(lua, &owlry)?;
    math::register_math_api(lua, &owlry)?;

    // Register Phase 4 APIs (hooks, actions, themes)
    hook::register_hook_api(lua, &owlry, plugin_id)?;
    action::register_action_api(lua, &owlry, plugin_id)?;
    theme::register_theme_api(lua, &owlry, plugin_id, plugin_dir)?;

    // Set owlry as global
    globals.set("owlry", owlry)?;

    Ok(())
}

/// Get provider registrations from the Lua runtime
///
/// Returns all providers that were registered via `owlry.provider.register()`
pub fn get_provider_registrations(lua: &Lua) -> LuaResult<Vec<ProviderRegistration>> {
    provider::get_registrations(lua)
}
