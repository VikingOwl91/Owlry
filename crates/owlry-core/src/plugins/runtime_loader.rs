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

use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use libloading::{Library, Symbol};
use owlry_plugin_api::{PluginItem, RStr, RString, RVec};

use super::error::{PluginError, PluginResult};
use crate::providers::{ItemSource, LaunchItem, Provider, ProviderType};

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

// SAFETY: The underlying runtime state (Lua VM, Rune VM) is Send — mlua enables
// the "send" feature and Rune wraps its state in Mutex internally. Access is always
// serialized through Arc<Mutex<RuntimeHandle>>, so there are no data races.
unsafe impl Send for RuntimeHandle {}

/// VTable for script runtime functions (used by both Lua and Rune)
#[repr(C)]
pub struct ScriptRuntimeVTable {
    pub info: extern "C" fn() -> RuntimeInfo,
    pub init: extern "C" fn(plugins_dir: RStr<'_>, owlry_version: RStr<'_>) -> RuntimeHandle,
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
    /// Keep library alive — wrapped in ManuallyDrop so we never call dlclose().
    /// dlclose() unmaps the library code; any thread-local destructors inside the
    /// library then SIGSEGV when they try to run against the unmapped addresses.
    /// Runtime libraries live for the process lifetime, so leaking the handle is safe.
    _library: ManuallyDrop<Arc<Library>>,
    /// Runtime vtable
    vtable: &'static ScriptRuntimeVTable,
    /// Runtime handle shared with all RuntimeProvider instances for this runtime.
    /// Mutex serializes concurrent vtable calls. Arc shares ownership so all
    /// RuntimeProviders can call into the runtime through the same handle.
    handle: Arc<Mutex<RuntimeHandle>>,
    /// Provider information
    providers: Vec<ScriptProviderInfo>,
}

impl LoadedRuntime {
    /// Load the Lua runtime from the system directory
    pub fn load_lua(plugins_dir: &Path, owlry_version: &str) -> PluginResult<Self> {
        Self::load_from_path(
            "Lua",
            &PathBuf::from(SYSTEM_RUNTIMES_DIR).join("liblua.so"),
            b"owlry_lua_runtime_vtable",
            plugins_dir,
            owlry_version,
        )
    }

    /// Load a runtime from a specific path
    fn load_from_path(
        name: &'static str,
        library_path: &Path,
        vtable_symbol: &[u8],
        plugins_dir: &Path,
        owlry_version: &str,
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
        let raw_handle = (vtable.init)(RStr::from_str(&plugins_dir_str), RStr::from_str(owlry_version));
        let handle = Arc::new(Mutex::new(raw_handle));

        // Get provider information — lock to serialize the vtable call
        let providers_rvec = {
            let h = handle.lock().unwrap();
            (vtable.providers)(*h)
        };
        let providers: Vec<ScriptProviderInfo> = providers_rvec.into_iter().collect();

        log::info!(
            "Loaded {} runtime with {} provider(s)",
            name,
            providers.len()
        );

        Ok(Self {
            name,
            _library: ManuallyDrop::new(library),
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
                let provider = RuntimeProvider::new(
                    self.name,
                    self.vtable,
                    Arc::clone(&self.handle),
                    info.clone(),
                );
                Box::new(provider) as Box<dyn Provider>
            })
            .collect()
    }
}

impl Drop for LoadedRuntime {
    fn drop(&mut self) {
        let h = self.handle.lock().unwrap();
        (self.vtable.drop)(*h);
        // Do NOT drop _library: ManuallyDrop ensures dlclose() is never called.
        // See field comment for rationale.
    }
}
// LoadedRuntime is Send + Sync because:
// - Arc<Mutex<RuntimeHandle>> is Send + Sync (RuntimeHandle: Send via unsafe impl above)
// - All other fields are 'static references or Send types
// No unsafe impl needed — this is derived automatically.

/// A provider backed by a dynamically loaded runtime
pub struct RuntimeProvider {
    /// Runtime name (for logging)
    #[allow(dead_code)]
    runtime_name: &'static str,
    vtable: &'static ScriptRuntimeVTable,
    /// Shared with the owning LoadedRuntime and sibling RuntimeProviders.
    /// Mutex serializes concurrent vtable calls on the same runtime handle.
    handle: Arc<Mutex<RuntimeHandle>>,
    info: ScriptProviderInfo,
    items: Vec<LaunchItem>,
}

impl RuntimeProvider {
    fn new(
        runtime_name: &'static str,
        vtable: &'static ScriptRuntimeVTable,
        handle: Arc<Mutex<RuntimeHandle>>,
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
            source: ItemSource::ScriptPlugin,
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
        let items_rvec = {
            let h = self.handle.lock().unwrap();
            (self.vtable.refresh)(*h, name_rstr)
        };
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

// RuntimeProvider is Send + Sync because:
// - Arc<Mutex<RuntimeHandle>> is Send + Sync (RuntimeHandle: Send via unsafe impl above)
// - vtable is &'static (Send + Sync), info and items are Send
// No unsafe impl needed — this is derived automatically.

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
    pub fn load_rune(plugins_dir: &Path, owlry_version: &str) -> PluginResult<Self> {
        Self::load_from_path(
            "Rune",
            &PathBuf::from(SYSTEM_RUNTIMES_DIR).join("librune.so"),
            b"owlry_rune_runtime_vtable",
            plugins_dir,
            owlry_version,
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
