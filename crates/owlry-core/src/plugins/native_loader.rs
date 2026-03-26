//! Native Plugin Loader
//!
//! Loads pre-compiled Rust plugins (.so files) from `/usr/lib/owlry/plugins/`.
//! These plugins use the ABI-stable interface defined in `owlry-plugin-api`.
//!
//! Note: This module is infrastructure for the plugin architecture. Full integration
//! with ProviderManager is pending Phase 5 (AUR Packaging) when native plugins
//! will actually be deployed.

#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};

use libloading::Library;
use log::{debug, error, info, warn};
use owlry_plugin_api::{
    API_VERSION, HostAPI, NotifyUrgency, PluginInfo, PluginVTable, ProviderHandle, ProviderInfo,
    ProviderKind, RStr,
};

use crate::notify;

// ============================================================================
// Host API Implementation
// ============================================================================

/// Host notification handler
extern "C" fn host_notify(
    summary: RStr<'_>,
    body: RStr<'_>,
    icon: RStr<'_>,
    urgency: NotifyUrgency,
) {
    let icon_str = icon.as_str();
    let icon_opt = if icon_str.is_empty() {
        None
    } else {
        Some(icon_str)
    };

    let notify_urgency = match urgency {
        NotifyUrgency::Low => notify::NotifyUrgency::Low,
        NotifyUrgency::Normal => notify::NotifyUrgency::Normal,
        NotifyUrgency::Critical => notify::NotifyUrgency::Critical,
    };

    notify::notify_with_options(summary.as_str(), body.as_str(), icon_opt, notify_urgency);
}

/// Host log info handler
extern "C" fn host_log_info(message: RStr<'_>) {
    info!("[plugin] {}", message.as_str());
}

/// Host log warning handler
extern "C" fn host_log_warn(message: RStr<'_>) {
    warn!("[plugin] {}", message.as_str());
}

/// Host log error handler
extern "C" fn host_log_error(message: RStr<'_>) {
    error!("[plugin] {}", message.as_str());
}

/// Static host API instance
static HOST_API: HostAPI = HostAPI {
    notify: host_notify,
    log_info: host_log_info,
    log_warn: host_log_warn,
    log_error: host_log_error,
};

/// Initialize the host API (called once before loading plugins)
static HOST_API_INIT: Once = Once::new();

fn ensure_host_api_initialized() {
    HOST_API_INIT.call_once(|| {
        // SAFETY: We only call this once, before any plugins are loaded
        unsafe {
            owlry_plugin_api::init_host_api(&HOST_API);
        }
        debug!("Host API initialized for plugins");
    });
}

use super::error::{PluginError, PluginResult};

/// Default directory for system-installed native plugins
pub const SYSTEM_PLUGINS_DIR: &str = "/usr/lib/owlry/plugins";

/// A loaded native plugin with its library handle and vtable
pub struct NativePlugin {
    /// Plugin metadata
    pub info: PluginInfo,
    /// List of providers this plugin offers
    pub providers: Vec<ProviderInfo>,
    /// The vtable for calling plugin functions
    vtable: &'static PluginVTable,
    /// The loaded library (must be kept alive)
    _library: Library,
}

impl NativePlugin {
    /// Get the plugin ID
    pub fn id(&self) -> &str {
        self.info.id.as_str()
    }

    /// Get the plugin name
    pub fn name(&self) -> &str {
        self.info.name.as_str()
    }

    /// Initialize a provider by ID
    pub fn init_provider(&self, provider_id: &str) -> ProviderHandle {
        (self.vtable.provider_init)(provider_id.into())
    }

    /// Refresh a static provider
    pub fn refresh_provider(&self, handle: ProviderHandle) -> Vec<owlry_plugin_api::PluginItem> {
        (self.vtable.provider_refresh)(handle).into_iter().collect()
    }

    /// Query a dynamic provider
    pub fn query_provider(
        &self,
        handle: ProviderHandle,
        query: &str,
    ) -> Vec<owlry_plugin_api::PluginItem> {
        (self.vtable.provider_query)(handle, query.into())
            .into_iter()
            .collect()
    }

    /// Drop a provider handle
    pub fn drop_provider(&self, handle: ProviderHandle) {
        (self.vtable.provider_drop)(handle)
    }
}

// SAFETY: NativePlugin is safe to send between threads because:
// - `info` and `providers` are plain data (RString, RVec from abi_stable are Send+Sync)
// - `vtable` is a &'static reference to immutable function pointers
// - `_library` (libloading::Library) is Send+Sync
unsafe impl Send for NativePlugin {}
unsafe impl Sync for NativePlugin {}

/// Manages native plugin discovery and loading
pub struct NativePluginLoader {
    /// Directory to scan for plugins
    plugins_dir: PathBuf,
    /// Loaded plugins by ID (Arc for shared ownership with providers)
    plugins: HashMap<String, Arc<NativePlugin>>,
    /// Plugin IDs that are disabled
    disabled: Vec<String>,
}

impl NativePluginLoader {
    /// Create a new loader with the default system plugins directory
    pub fn new() -> Self {
        Self::with_dir(PathBuf::from(SYSTEM_PLUGINS_DIR))
    }

    /// Create a new loader with a custom plugins directory
    pub fn with_dir(plugins_dir: PathBuf) -> Self {
        Self {
            plugins_dir,
            plugins: HashMap::new(),
            disabled: Vec::new(),
        }
    }

    /// Set the list of disabled plugin IDs
    pub fn set_disabled(&mut self, disabled: Vec<String>) {
        self.disabled = disabled;
    }

    /// Check if the plugins directory exists
    pub fn plugins_dir_exists(&self) -> bool {
        self.plugins_dir.exists()
    }

    /// Discover and load all native plugins
    pub fn discover(&mut self) -> PluginResult<usize> {
        // Initialize host API before loading any plugins
        ensure_host_api_initialized();

        if !self.plugins_dir.exists() {
            debug!(
                "Native plugins directory does not exist: {}",
                self.plugins_dir.display()
            );
            return Ok(0);
        }

        info!(
            "Discovering native plugins in {}",
            self.plugins_dir.display()
        );

        let entries = std::fs::read_dir(&self.plugins_dir).map_err(|e| {
            PluginError::LoadError(format!(
                "Failed to read plugins directory {}: {}",
                self.plugins_dir.display(),
                e
            ))
        })?;

        let mut loaded_count = 0;

        for entry in entries.flatten() {
            let path = entry.path();

            // Only process .so files
            if path.extension() != Some(OsStr::new("so")) {
                continue;
            }

            match self.load_plugin(&path) {
                Ok(plugin) => {
                    let id = plugin.id().to_string();

                    // Check if disabled
                    if self.disabled.contains(&id) {
                        info!("Native plugin '{}' is disabled, skipping", id);
                        continue;
                    }

                    info!(
                        "Loaded native plugin '{}' v{} with {} providers",
                        plugin.name(),
                        plugin.info.version.as_str(),
                        plugin.providers.len()
                    );

                    self.plugins.insert(id, Arc::new(plugin));
                    loaded_count += 1;
                }
                Err(e) => {
                    error!("Failed to load plugin {:?}: {}", path, e);
                }
            }
        }

        info!("Loaded {} native plugins", loaded_count);
        Ok(loaded_count)
    }

    /// Load a single plugin from a .so file
    fn load_plugin(&self, path: &Path) -> PluginResult<NativePlugin> {
        debug!("Loading native plugin from {:?}", path);

        // Load the library
        // SAFETY: We trust plugins in /usr/lib/owlry/plugins/ as they were
        // installed by the package manager
        let library = unsafe { Library::new(path) }.map_err(|e| {
            PluginError::LoadError(format!("Failed to load library {:?}: {}", path, e))
        })?;

        // Get the vtable function
        let vtable: &'static PluginVTable = unsafe {
            let func: libloading::Symbol<extern "C" fn() -> &'static PluginVTable> =
                library.get(b"owlry_plugin_vtable").map_err(|e| {
                    PluginError::LoadError(format!(
                        "Plugin {:?} missing owlry_plugin_vtable symbol: {}",
                        path, e
                    ))
                })?;
            func()
        };

        // Get plugin info
        let info = (vtable.info)();

        // Check API version compatibility
        if info.api_version != API_VERSION {
            return Err(PluginError::LoadError(format!(
                "Plugin '{}' has API version {} but owlry requires version {}",
                info.id.as_str(),
                info.api_version,
                API_VERSION
            )));
        }

        // Get provider list
        let providers: Vec<ProviderInfo> = (vtable.providers)().into_iter().collect();

        Ok(NativePlugin {
            info,
            providers,
            vtable,
            _library: library,
        })
    }

    /// Get a loaded plugin by ID
    pub fn get(&self, id: &str) -> Option<Arc<NativePlugin>> {
        self.plugins.get(id).cloned()
    }

    /// Get all loaded plugins as Arc references
    pub fn plugins(&self) -> impl Iterator<Item = Arc<NativePlugin>> + '_ {
        self.plugins.values().cloned()
    }

    /// Get all loaded plugins as a Vec (for passing to create_providers)
    pub fn into_plugins(self) -> Vec<Arc<NativePlugin>> {
        self.plugins.into_values().collect()
    }

    /// Get the number of loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Create providers from all loaded native plugins
    ///
    /// Returns a vec of (plugin_id, provider_info, handle) tuples that can be
    /// used to create NativeProvider instances.
    pub fn create_provider_handles(&self) -> Vec<(String, ProviderInfo, ProviderHandle)> {
        let mut handles = Vec::new();

        for plugin in self.plugins.values() {
            for provider_info in &plugin.providers {
                let handle = plugin.init_provider(provider_info.id.as_str());
                handles.push((plugin.id().to_string(), provider_info.clone(), handle));
            }
        }

        handles
    }
}

impl Default for NativePluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Active provider instance from a native plugin
pub struct NativeProviderInstance {
    /// Plugin ID this provider belongs to
    pub plugin_id: String,
    /// Provider metadata
    pub info: ProviderInfo,
    /// Handle to the provider state
    pub handle: ProviderHandle,
    /// Cached items for static providers
    pub cached_items: Vec<owlry_plugin_api::PluginItem>,
}

impl NativeProviderInstance {
    /// Create a new provider instance
    pub fn new(plugin_id: String, info: ProviderInfo, handle: ProviderHandle) -> Self {
        Self {
            plugin_id,
            info,
            handle,
            cached_items: Vec::new(),
        }
    }

    /// Check if this is a static provider
    pub fn is_static(&self) -> bool {
        self.info.provider_type == ProviderKind::Static
    }

    /// Check if this is a dynamic provider
    pub fn is_dynamic(&self) -> bool {
        self.info.provider_type == ProviderKind::Dynamic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_nonexistent_dir() {
        let mut loader = NativePluginLoader::with_dir(PathBuf::from("/nonexistent/path"));
        let count = loader.discover().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_loader_empty_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut loader = NativePluginLoader::with_dir(temp.path().to_path_buf());
        let count = loader.discover().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_disabled_plugins() {
        let mut loader = NativePluginLoader::new();
        loader.set_disabled(vec!["test-plugin".to_string()]);
        assert!(loader.disabled.contains(&"test-plugin".to_string()));
    }
}
