//! Owlry Lua Runtime
//!
//! This crate provides Lua plugin support for owlry. It is loaded dynamically
//! by the core when Lua plugins need to be executed.
//!
//! # Architecture
//!
//! The runtime acts as a "meta-plugin" that:
//! 1. Discovers Lua plugins in `~/.config/owlry/plugins/`
//! 2. Creates sandboxed Lua VMs for each plugin
//! 3. Registers the `owlry` API table
//! 4. Bridges Lua providers to native `PluginItem` format
//!
//! # Plugin Structure
//!
//! Each plugin lives in its own directory:
//! ```text
//! ~/.config/owlry/plugins/
//!   my-plugin/
//!     plugin.toml    # Plugin manifest
//!     init.lua       # Entry point
//! ```

mod api;
mod loader;
mod manifest;
mod runtime;

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{PluginItem, ProviderKind};
use std::collections::HashMap;
use std::path::PathBuf;

use loader::LoadedPlugin;

// Runtime metadata
const RUNTIME_ID: &str = "lua";
const RUNTIME_NAME: &str = "Lua Runtime";
const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");
const RUNTIME_DESCRIPTION: &str = "Lua 5.4 runtime for user plugins";

/// API version for compatibility checking
pub const LUA_RUNTIME_API_VERSION: u32 = 1;

/// Runtime vtable - exported interface for the core to use
#[repr(C)]
pub struct LuaRuntimeVTable {
    /// Get runtime info
    pub info: extern "C" fn() -> RuntimeInfo,
    /// Initialize the runtime with plugins directory
    pub init: extern "C" fn(plugins_dir: RStr<'_>) -> RuntimeHandle,
    /// Get provider infos from all loaded plugins
    pub providers: extern "C" fn(handle: RuntimeHandle) -> RVec<LuaProviderInfo>,
    /// Refresh a provider's items
    pub refresh: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem>,
    /// Query a dynamic provider
    pub query: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>, query: RStr<'_>) -> RVec<PluginItem>,
    /// Cleanup and drop the runtime
    pub drop: extern "C" fn(handle: RuntimeHandle),
}

/// Runtime info returned by the runtime
#[repr(C)]
pub struct RuntimeInfo {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub description: RString,
    pub api_version: u32,
}

/// Opaque handle to the runtime state
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuntimeHandle {
    pub ptr: *mut (),
}

unsafe impl Send for RuntimeHandle {}
unsafe impl Sync for RuntimeHandle {}

impl RuntimeHandle {
    /// Create a null handle (reserved for error cases)
    #[allow(dead_code)]
    fn null() -> Self {
        Self { ptr: std::ptr::null_mut() }
    }

    fn from_box<T>(state: Box<T>) -> Self {
        Self { ptr: Box::into_raw(state) as *mut () }
    }

    unsafe fn drop_as<T>(&self) {
        if !self.ptr.is_null() {
            unsafe { drop(Box::from_raw(self.ptr as *mut T)) };
        }
    }
}

/// Provider info from a Lua plugin
#[repr(C)]
pub struct LuaProviderInfo {
    /// Full provider ID: "plugin_id:provider_name"
    pub id: RString,
    /// Plugin ID this provider belongs to
    pub plugin_id: RString,
    /// Provider name within the plugin
    pub provider_name: RString,
    /// Display name
    pub display_name: RString,
    /// Optional prefix trigger
    pub prefix: ROption<RString>,
    /// Icon name
    pub icon: RString,
    /// Provider type (static/dynamic)
    pub provider_type: ProviderKind,
    /// Type ID for filtering
    pub type_id: RString,
}

/// Internal runtime state
struct LuaRuntimeState {
    plugins_dir: PathBuf,
    plugins: HashMap<String, LoadedPlugin>,
    /// Maps "plugin_id:provider_name" to plugin_id for lookup
    provider_map: HashMap<String, String>,
}

impl LuaRuntimeState {
    fn new(plugins_dir: PathBuf) -> Self {
        Self {
            plugins_dir,
            plugins: HashMap::new(),
            provider_map: HashMap::new(),
        }
    }

    fn discover_and_load(&mut self, owlry_version: &str) {
        let discovered = match loader::discover_plugins(&self.plugins_dir) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("owlry-lua: Failed to discover plugins: {}", e);
                return;
            }
        };

        for (id, (manifest, path)) in discovered {
            // Check version compatibility
            if !manifest.is_compatible_with(owlry_version) {
                eprintln!("owlry-lua: Plugin '{}' not compatible with owlry {}", id, owlry_version);
                continue;
            }

            let mut plugin = LoadedPlugin::new(manifest, path);
            if let Err(e) = plugin.initialize() {
                eprintln!("owlry-lua: Failed to initialize plugin '{}': {}", id, e);
                continue;
            }

            // Build provider map
            if let Ok(registrations) = plugin.get_provider_registrations() {
                for reg in &registrations {
                    let full_id = format!("{}:{}", id, reg.name);
                    self.provider_map.insert(full_id, id.clone());
                }
            }

            self.plugins.insert(id, plugin);
        }
    }

    fn get_providers(&self) -> Vec<LuaProviderInfo> {
        let mut providers = Vec::new();

        for (plugin_id, plugin) in &self.plugins {
            if let Ok(registrations) = plugin.get_provider_registrations() {
                for reg in registrations {
                    let full_id = format!("{}:{}", plugin_id, reg.name);
                    let provider_type = if reg.is_dynamic {
                        ProviderKind::Dynamic
                    } else {
                        ProviderKind::Static
                    };

                    providers.push(LuaProviderInfo {
                        id: RString::from(full_id),
                        plugin_id: RString::from(plugin_id.as_str()),
                        provider_name: RString::from(reg.name.as_str()),
                        display_name: RString::from(reg.display_name.as_str()),
                        prefix: reg.prefix.map(RString::from).into(),
                        icon: RString::from(reg.default_icon.as_str()),
                        provider_type,
                        type_id: RString::from(reg.type_id.as_str()),
                    });
                }
            }
        }

        providers
    }

    fn refresh_provider(&self, provider_id: &str) -> Vec<PluginItem> {
        // Parse "plugin_id:provider_name"
        let parts: Vec<&str> = provider_id.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Vec::new();
        }
        let (plugin_id, provider_name) = (parts[0], parts[1]);

        if let Some(plugin) = self.plugins.get(plugin_id) {
            match plugin.call_provider_refresh(provider_name) {
                Ok(items) => items,
                Err(e) => {
                    eprintln!("owlry-lua: Refresh failed for {}: {}", provider_id, e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    }

    fn query_provider(&self, provider_id: &str, query: &str) -> Vec<PluginItem> {
        // Parse "plugin_id:provider_name"
        let parts: Vec<&str> = provider_id.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Vec::new();
        }
        let (plugin_id, provider_name) = (parts[0], parts[1]);

        if let Some(plugin) = self.plugins.get(plugin_id) {
            match plugin.call_provider_query(provider_name, query) {
                Ok(items) => items,
                Err(e) => {
                    eprintln!("owlry-lua: Query failed for {}: {}", provider_id, e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    }
}

// ============================================================================
// Exported Functions
// ============================================================================

extern "C" fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        id: RString::from(RUNTIME_ID),
        name: RString::from(RUNTIME_NAME),
        version: RString::from(RUNTIME_VERSION),
        description: RString::from(RUNTIME_DESCRIPTION),
        api_version: LUA_RUNTIME_API_VERSION,
    }
}

extern "C" fn runtime_init(plugins_dir: RStr<'_>) -> RuntimeHandle {
    let plugins_dir = PathBuf::from(plugins_dir.as_str());
    let mut state = Box::new(LuaRuntimeState::new(plugins_dir));

    // TODO: Get owlry version from core somehow
    // For now, use a reasonable default
    state.discover_and_load("0.3.0");

    RuntimeHandle::from_box(state)
}

extern "C" fn runtime_providers(handle: RuntimeHandle) -> RVec<LuaProviderInfo> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let state = unsafe { &*(handle.ptr as *const LuaRuntimeState) };
    state.get_providers().into()
}

extern "C" fn runtime_refresh(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let state = unsafe { &*(handle.ptr as *const LuaRuntimeState) };
    state.refresh_provider(provider_id.as_str()).into()
}

extern "C" fn runtime_query(handle: RuntimeHandle, provider_id: RStr<'_>, query: RStr<'_>) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let state = unsafe { &*(handle.ptr as *const LuaRuntimeState) };
    state.query_provider(provider_id.as_str(), query.as_str()).into()
}

extern "C" fn runtime_drop(handle: RuntimeHandle) {
    if !handle.ptr.is_null() {
        unsafe {
            handle.drop_as::<LuaRuntimeState>();
        }
    }
}

/// Static vtable instance
static LUA_RUNTIME_VTABLE: LuaRuntimeVTable = LuaRuntimeVTable {
    info: runtime_info,
    init: runtime_init,
    providers: runtime_providers,
    refresh: runtime_refresh,
    query: runtime_query,
    drop: runtime_drop,
};

/// Entry point - returns the runtime vtable
#[unsafe(no_mangle)]
pub extern "C" fn owlry_lua_runtime_vtable() -> &'static LuaRuntimeVTable {
    &LUA_RUNTIME_VTABLE
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_info() {
        let info = runtime_info();
        assert_eq!(info.id.as_str(), "lua");
        assert_eq!(info.api_version, LUA_RUNTIME_API_VERSION);
    }

    #[test]
    fn test_runtime_handle_null() {
        let handle = RuntimeHandle::null();
        assert!(handle.ptr.is_null());
    }

    #[test]
    fn test_runtime_handle_from_box() {
        let state = Box::new(42u32);
        let handle = RuntimeHandle::from_box(state);
        assert!(!handle.ptr.is_null());
        unsafe { handle.drop_as::<u32>() };
    }
}
