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
use owlry_plugin_api::PluginItem;
use std::collections::HashMap;
use std::path::PathBuf;

use loader::LoadedPlugin;

/// Runtime vtable - exported interface for the core to use
#[repr(C)]
pub struct LuaRuntimeVTable {
    /// Get runtime info
    pub info: extern "C" fn() -> RuntimeInfo,
    /// Initialize the runtime with plugins directory
    pub init: extern "C" fn(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle,
    /// Get provider infos from all loaded plugins
    pub providers: extern "C" fn(handle: RuntimeHandle) -> RVec<LuaProviderInfo>,
    /// Refresh a provider's items
    pub refresh: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem>,
    /// Query a dynamic provider
    pub query: extern "C" fn(
        handle: RuntimeHandle,
        provider_id: RStr<'_>,
        query: RStr<'_>,
    ) -> RVec<PluginItem>,
    /// Cleanup and drop the runtime
    pub drop: extern "C" fn(handle: RuntimeHandle),
}

/// Runtime info returned by the runtime
#[repr(C)]
pub struct RuntimeInfo {
    pub name: RString,
    pub version: RString,
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
        Self {
            ptr: std::ptr::null_mut(),
        }
    }

    fn from_box<T>(state: Box<T>) -> Self {
        Self {
            ptr: Box::into_raw(state) as *mut (),
        }
    }

    unsafe fn drop_as<T>(&self) {
        if !self.ptr.is_null() {
            unsafe { drop(Box::from_raw(self.ptr as *mut T)) };
        }
    }
}

/// Provider info from a Lua plugin
///
/// Must match ScriptProviderInfo layout in owlry-core/src/plugins/runtime_loader.rs
#[repr(C)]
pub struct LuaProviderInfo {
    /// Provider name (used as vtable refresh/query key: "plugin_id:provider_name")
    pub name: RString,
    /// Display name
    pub display_name: RString,
    /// Type ID for filtering
    pub type_id: RString,
    /// Icon name
    pub default_icon: RString,
    /// Whether this is a static provider (true) or dynamic (false)
    pub is_static: bool,
    /// Optional prefix trigger
    pub prefix: ROption<RString>,
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
                eprintln!(
                    "owlry-lua: Plugin '{}' not compatible with owlry {}",
                    id, owlry_version
                );
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

                    providers.push(LuaProviderInfo {
                        name: RString::from(full_id),
                        display_name: RString::from(reg.display_name.as_str()),
                        type_id: RString::from(reg.type_id.as_str()),
                        default_icon: RString::from(reg.default_icon.as_str()),
                        is_static: !reg.is_dynamic,
                        prefix: reg.prefix.map(RString::from).into(),
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
        name: RString::from("Lua"),
        version: RString::from(env!("CARGO_PKG_VERSION")),
    }
}

extern "C" fn runtime_init(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle {
    let plugins_dir = PathBuf::from(plugins_dir.as_str());
    let mut state = Box::new(LuaRuntimeState::new(plugins_dir));
    state.discover_and_load(owlry_version.as_str());
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

extern "C" fn runtime_query(
    handle: RuntimeHandle,
    provider_id: RStr<'_>,
    query: RStr<'_>,
) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let state = unsafe { &*(handle.ptr as *const LuaRuntimeState) };
    state
        .query_provider(provider_id.as_str(), query.as_str())
        .into()
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
        assert_eq!(info.name.as_str(), "Lua");
        assert!(!info.version.as_str().is_empty());
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
