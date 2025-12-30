//! Owlry Rune Runtime
//!
//! This crate provides a Rune scripting runtime for owlry user plugins.
//! It is loaded dynamically by the core when installed.
//!
//! # Architecture
//!
//! The runtime exports a C-compatible vtable that the core uses to:
//! 1. Initialize the runtime with a plugins directory
//! 2. Get a list of providers from loaded plugins
//! 3. Refresh/query providers
//! 4. Clean up resources
//!
//! # Plugin Structure
//!
//! Rune plugins live in `~/.config/owlry/plugins/<plugin-name>/`:
//! ```text
//! my-plugin/
//!   plugin.toml    # Manifest
//!   init.rn        # Entry point (Rune script)
//! ```

mod api;
mod loader;
mod manifest;
mod runtime;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use owlry_plugin_api::{PluginItem, ROption, RStr, RString, RVec};

pub use loader::LoadedPlugin;
pub use manifest::PluginManifest;

// ============================================================================
// Runtime VTable (C-compatible interface)
// ============================================================================

/// Information about this runtime
#[repr(C)]
pub struct RuntimeInfo {
    pub name: RString,
    pub version: RString,
}

/// Information about a provider from a plugin
#[repr(C)]
#[derive(Clone)]
pub struct RuneProviderInfo {
    pub name: RString,
    pub display_name: RString,
    pub type_id: RString,
    pub default_icon: RString,
    pub is_static: bool,
    pub prefix: ROption<RString>,
}

/// Opaque handle to runtime state
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct RuntimeHandle(pub *mut ());

/// Runtime state managed by the handle
struct RuntimeState {
    plugins: HashMap<String, LoadedPlugin>,
    providers: Vec<RuneProviderInfo>,
}

/// VTable for the Rune runtime
#[repr(C)]
pub struct RuneRuntimeVTable {
    pub info: extern "C" fn() -> RuntimeInfo,
    pub init: extern "C" fn(plugins_dir: RStr<'_>) -> RuntimeHandle,
    pub providers: extern "C" fn(handle: RuntimeHandle) -> RVec<RuneProviderInfo>,
    pub refresh: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem>,
    pub query: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>, query: RStr<'_>) -> RVec<PluginItem>,
    pub drop: extern "C" fn(handle: RuntimeHandle),
}

// ============================================================================
// VTable Implementation
// ============================================================================

extern "C" fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        name: RString::from("rune"),
        version: RString::from(env!("CARGO_PKG_VERSION")),
    }
}

extern "C" fn runtime_init(plugins_dir: RStr<'_>) -> RuntimeHandle {
    let _ = env_logger::try_init();

    let plugins_dir = PathBuf::from(plugins_dir.as_str());
    log::info!("Initializing Rune runtime with plugins from: {}", plugins_dir.display());

    let mut state = RuntimeState {
        plugins: HashMap::new(),
        providers: Vec::new(),
    };

    // Discover and load Rune plugins
    match loader::discover_rune_plugins(&plugins_dir) {
        Ok(plugins) => {
            for (id, plugin) in plugins {
                // Collect provider info before storing plugin
                for reg in plugin.provider_registrations() {
                    state.providers.push(RuneProviderInfo {
                        name: RString::from(reg.name.as_str()),
                        display_name: RString::from(reg.display_name.as_str()),
                        type_id: RString::from(reg.type_id.as_str()),
                        default_icon: RString::from(reg.default_icon.as_str()),
                        is_static: reg.is_static,
                        prefix: reg.prefix.as_ref()
                            .map(|p| RString::from(p.as_str()))
                            .into(),
                    });
                }
                state.plugins.insert(id, plugin);
            }
            log::info!("Loaded {} Rune plugin(s) with {} provider(s)",
                      state.plugins.len(), state.providers.len());
        }
        Err(e) => {
            log::error!("Failed to discover Rune plugins: {}", e);
        }
    }

    // Box and leak the state, returning an opaque handle
    let boxed = Box::new(Mutex::new(state));
    RuntimeHandle(Box::into_raw(boxed) as *mut ())
}

extern "C" fn runtime_providers(handle: RuntimeHandle) -> RVec<RuneProviderInfo> {
    let state = unsafe { &*(handle.0 as *const Mutex<RuntimeState>) };
    let guard = state.lock().unwrap();
    guard.providers.clone().into_iter().collect()
}

extern "C" fn runtime_refresh(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem> {
    let state = unsafe { &*(handle.0 as *const Mutex<RuntimeState>) };
    let mut guard = state.lock().unwrap();

    let provider_name = provider_id.as_str();

    // Find the plugin that provides this provider
    for plugin in guard.plugins.values_mut() {
        if plugin.provides_provider(provider_name) {
            match plugin.refresh_provider(provider_name) {
                Ok(items) => return items.into_iter().collect(),
                Err(e) => {
                    log::error!("Failed to refresh provider '{}': {}", provider_name, e);
                    return RVec::new();
                }
            }
        }
    }

    log::warn!("Provider '{}' not found", provider_name);
    RVec::new()
}

extern "C" fn runtime_query(
    handle: RuntimeHandle,
    provider_id: RStr<'_>,
    query: RStr<'_>,
) -> RVec<PluginItem> {
    let state = unsafe { &*(handle.0 as *const Mutex<RuntimeState>) };
    let mut guard = state.lock().unwrap();

    let provider_name = provider_id.as_str();
    let query_str = query.as_str();

    // Find the plugin that provides this provider
    for plugin in guard.plugins.values_mut() {
        if plugin.provides_provider(provider_name) {
            match plugin.query_provider(provider_name, query_str) {
                Ok(items) => return items.into_iter().collect(),
                Err(e) => {
                    log::error!("Failed to query provider '{}': {}", provider_name, e);
                    return RVec::new();
                }
            }
        }
    }

    log::warn!("Provider '{}' not found", provider_name);
    RVec::new()
}

extern "C" fn runtime_drop(handle: RuntimeHandle) {
    if !handle.0.is_null() {
        // SAFETY: We created this box in runtime_init
        unsafe {
            let _ = Box::from_raw(handle.0 as *mut Mutex<RuntimeState>);
        }
        log::info!("Rune runtime cleaned up");
    }
}

/// Static vtable instance
static RUNE_RUNTIME_VTABLE: RuneRuntimeVTable = RuneRuntimeVTable {
    info: runtime_info,
    init: runtime_init,
    providers: runtime_providers,
    refresh: runtime_refresh,
    query: runtime_query,
    drop: runtime_drop,
};

/// Entry point - returns the runtime vtable
#[unsafe(no_mangle)]
pub extern "C" fn owlry_rune_runtime_vtable() -> &'static RuneRuntimeVTable {
    &RUNE_RUNTIME_VTABLE
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
        assert_eq!(info.name.as_str(), "rune");
        assert!(!info.version.as_str().is_empty());
    }

    #[test]
    fn test_runtime_lifecycle() {
        // Create a temp directory for plugins
        let temp = tempfile::TempDir::new().unwrap();
        let plugins_dir = temp.path().to_string_lossy();

        // Initialize runtime
        let handle = runtime_init(RStr::from_str(&plugins_dir));
        assert!(!handle.0.is_null());

        // Get providers (should be empty with no plugins)
        let providers = runtime_providers(handle);
        assert!(providers.is_empty());

        // Clean up
        runtime_drop(handle);
    }
}
