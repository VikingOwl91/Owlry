//! Dynamic runtime loader
//!
//! This module provides dynamic loading of script runtimes (Lua, Rune)
//! when they're not compiled into the core binary.
//!
//! Runtimes are loaded from `/usr/lib/owlry/runtimes/`:
//! - `liblua.so` - Lua runtime (from owlry-lua package)
//! - `librune.so` - Rune runtime (from owlry-rune package)
//!
//! Note: This module is infrastructure for the runtime architecture. Full integration
//! is pending Phase 5 (AUR Packaging) when runtime packages will be available.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::{Library, Symbol};
use owlry_plugin_api::{PluginItem, RStr, RString, RVec};

use super::error::{PluginError, PluginResult};
use crate::providers::{LaunchItem, Provider, ProviderType};

/// System directory for runtime libraries
pub const SYSTEM_RUNTIMES_DIR: &str = "/usr/lib/owlry/runtimes";

/// Information about a loaded runtime
#[repr(C)]
#[derive(Debug)]
pub struct RuntimeInfo {
    pub name: RString,
    pub version: RString,
}

/// Information about a provider from a script runtime
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ScriptProviderInfo {
    pub name: RString,
    pub display_name: RString,
    pub type_id: RString,
    pub default_icon: RString,
    pub is_static: bool,
    pub prefix: owlry_plugin_api::ROption<RString>,
}

// Type alias for backwards compatibility
pub type LuaProviderInfo = ScriptProviderInfo;

/// Handle to runtime-managed state
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct RuntimeHandle(pub *mut ());

/// VTable for script runtime functions (used by both Lua and Rune)
#[repr(C)]
pub struct ScriptRuntimeVTable {
    pub info: extern "C" fn() -> RuntimeInfo,
    pub init: extern "C" fn(plugins_dir: RStr<'_>) -> RuntimeHandle,
    pub providers: extern "C" fn(handle: RuntimeHandle) -> RVec<ScriptProviderInfo>,
    pub refresh: extern "C" fn(handle: RuntimeHandle, provider_id: RStr<'_>) -> RVec<PluginItem>,
    pub query: extern "C" fn(
        handle: RuntimeHandle,
        provider_id: RStr<'_>,
        query: RStr<'_>,
    ) -> RVec<PluginItem>,
    pub drop: extern "C" fn(handle: RuntimeHandle),
}

/// A loaded script runtime
pub struct LoadedRuntime {
    /// Runtime name (for logging)
    name: &'static str,
    /// Keep library alive
    _library: Arc<Library>,
    /// Runtime vtable
    vtable: &'static ScriptRuntimeVTable,
    /// Runtime handle (state)
    handle: RuntimeHandle,
    /// Provider information
    providers: Vec<ScriptProviderInfo>,
}

impl LoadedRuntime {
    /// Load the Lua runtime from the system directory
    pub fn load_lua(plugins_dir: &Path) -> PluginResult<Self> {
        Self::load_from_path(
            "Lua",
            &PathBuf::from(SYSTEM_RUNTIMES_DIR).join("liblua.so"),
            b"owlry_lua_runtime_vtable",
            plugins_dir,
        )
    }

    /// Load a runtime from a specific path
    fn load_from_path(
        name: &'static str,
        library_path: &Path,
        vtable_symbol: &[u8],
        plugins_dir: &Path,
    ) -> PluginResult<Self> {
        if !library_path.exists() {
            return Err(PluginError::NotFound(library_path.display().to_string()));
        }

        // SAFETY: We trust the runtime library to be correct
        let library = unsafe { Library::new(library_path) }
            .map_err(|e| PluginError::LoadError(format!("{}: {}", library_path.display(), e)))?;

        let library = Arc::new(library);

        // Get the vtable
        let vtable: &'static ScriptRuntimeVTable = unsafe {
            let get_vtable: Symbol<extern "C" fn() -> &'static ScriptRuntimeVTable> =
                library.get(vtable_symbol).map_err(|e| {
                    PluginError::LoadError(format!(
                        "{}: Missing vtable symbol: {}",
                        library_path.display(),
                        e
                    ))
                })?;
            get_vtable()
        };

        // Initialize the runtime
        let plugins_dir_str = plugins_dir.to_string_lossy();
        let handle = (vtable.init)(RStr::from_str(&plugins_dir_str));

        // Get provider information
        let providers_rvec = (vtable.providers)(handle);
        let providers: Vec<ScriptProviderInfo> = providers_rvec.into_iter().collect();

        log::info!(
            "Loaded {} runtime with {} provider(s)",
            name,
            providers.len()
        );

        Ok(Self {
            name,
            _library: library,
            vtable,
            handle,
            providers,
        })
    }

    /// Get all providers from this runtime
    pub fn providers(&self) -> &[ScriptProviderInfo] {
        &self.providers
    }

    /// Create Provider trait objects for all providers in this runtime
    pub fn create_providers(&self) -> Vec<Box<dyn Provider>> {
        self.providers
            .iter()
            .map(|info| {
                let provider =
                    RuntimeProvider::new(self.name, self.vtable, self.handle, info.clone());
                Box::new(provider) as Box<dyn Provider>
            })
            .collect()
    }
}

impl Drop for LoadedRuntime {
    fn drop(&mut self) {
        (self.vtable.drop)(self.handle);
    }
}

/// A provider backed by a dynamically loaded runtime
pub struct RuntimeProvider {
    /// Runtime name (for logging)
    #[allow(dead_code)]
    runtime_name: &'static str,
    vtable: &'static ScriptRuntimeVTable,
    handle: RuntimeHandle,
    info: ScriptProviderInfo,
    items: Vec<LaunchItem>,
}

impl RuntimeProvider {
    fn new(
        runtime_name: &'static str,
        vtable: &'static ScriptRuntimeVTable,
        handle: RuntimeHandle,
        info: ScriptProviderInfo,
    ) -> Self {
        Self {
            runtime_name,
            vtable,
            handle,
            info,
            items: Vec::new(),
        }
    }

    fn convert_item(&self, item: PluginItem) -> LaunchItem {
        LaunchItem {
            id: item.id.to_string(),
            name: item.name.to_string(),
            description: item.description.into_option().map(|s| s.to_string()),
            icon: item.icon.into_option().map(|s| s.to_string()),
            provider: ProviderType::Plugin(self.info.type_id.to_string()),
            command: item.command.to_string(),
            terminal: item.terminal,
            tags: item.keywords.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl Provider for RuntimeProvider {
    fn name(&self) -> &str {
        self.info.name.as_str()
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(self.info.type_id.to_string())
    }

    fn refresh(&mut self) {
        if !self.info.is_static {
            return;
        }

        let name_rstr = RStr::from_str(self.info.name.as_str());
        let items_rvec = (self.vtable.refresh)(self.handle, name_rstr);
        self.items = items_rvec
            .into_iter()
            .map(|i| self.convert_item(i))
            .collect();

        log::debug!(
            "[RuntimeProvider] '{}' refreshed with {} items",
            self.info.name,
            self.items.len()
        );
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

// RuntimeProvider needs to be Send + Sync for the Provider trait.
// Safety: RuntimeHandle is an opaque FFI handle accessed only through
// extern "C" vtable functions. The same safety argument that justifies
// Send applies to Sync — all access is mediated by the vtable.
unsafe impl Send for RuntimeProvider {}
unsafe impl Sync for RuntimeProvider {}

/// Check if the Lua runtime is available
pub fn lua_runtime_available() -> bool {
    PathBuf::from(SYSTEM_RUNTIMES_DIR)
        .join("liblua.so")
        .exists()
}

/// Check if the Rune runtime is available
pub fn rune_runtime_available() -> bool {
    PathBuf::from(SYSTEM_RUNTIMES_DIR)
        .join("librune.so")
        .exists()
}

impl LoadedRuntime {
    /// Load the Rune runtime from the system directory
    pub fn load_rune(plugins_dir: &Path) -> PluginResult<Self> {
        Self::load_from_path(
            "Rune",
            &PathBuf::from(SYSTEM_RUNTIMES_DIR).join("librune.so"),
            b"owlry_rune_runtime_vtable",
            plugins_dir,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_runtime_check_doesnt_panic() {
        // Just verify the function runs without panicking
        // Result depends on whether runtime is installed
        let _available = lua_runtime_available();
    }

    #[test]
    fn test_rune_runtime_check_doesnt_panic() {
        // Just verify the function runs without panicking
        // Result depends on whether runtime is installed
        let _available = rune_runtime_available();
    }
}
