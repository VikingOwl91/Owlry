//! Owlry Plugin System
//!
//! This module provides plugin support for extending owlry's functionality.
//! Plugins can register providers, actions, themes, and hooks.
//!
//! # Plugin Types
//!
//! - **Native plugins** (.so): Pre-compiled Rust plugins loaded from `/usr/lib/owlry/plugins/`
//! - **Lua plugins**: Script-based plugins from `~/.config/owlry/plugins/` (requires `lua` feature)
//!
//! # Plugin Structure (Lua)
//!
//! Each Lua plugin lives in its own directory under `~/.config/owlry/plugins/`:
//!
//! ```text
//! ~/.config/owlry/plugins/
//!   my-plugin/
//!     plugin.toml    # Plugin manifest
//!     init.lua       # Entry point
//!     lib/           # Optional modules
//! ```

// Always available
pub mod error;
pub mod manifest;
pub mod native_loader;
pub mod registry;
pub mod runtime_loader;

// Lua-specific modules (require mlua)
#[cfg(feature = "lua")]
pub mod api;
#[cfg(feature = "lua")]
pub mod loader;
#[cfg(feature = "lua")]
pub mod runtime;

// Re-export commonly used types
#[cfg(feature = "lua")]
pub use api::provider::{PluginItem, ProviderRegistration};
#[cfg(feature = "lua")]
#[allow(unused_imports)]
pub use api::{ActionRegistration, HookEvent, ThemeRegistration};

#[allow(unused_imports)]
pub use error::{PluginError, PluginResult};

#[cfg(feature = "lua")]
pub use loader::LoadedPlugin;

// Used by plugins/commands.rs for plugin CLI commands
#[allow(unused_imports)]
pub use manifest::{PluginManifest, check_compatibility, discover_plugins};

// ============================================================================
// Lua Plugin Manager (only available with lua feature)
// ============================================================================

#[cfg(feature = "lua")]
mod lua_manager {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::rc::Rc;

    use manifest::{check_compatibility, discover_plugins};

    /// Plugin manager coordinates loading, initialization, and lifecycle of Lua plugins
    pub struct PluginManager {
        /// Directory where plugins are stored
        plugins_dir: PathBuf,
        /// Current owlry version for compatibility checks
        owlry_version: String,
        /// Loaded plugins by ID (Rc<RefCell<>> allows sharing with LuaProviders)
        plugins: HashMap<String, Rc<RefCell<LoadedPlugin>>>,
        /// Plugin IDs that are explicitly disabled
        disabled: Vec<String>,
    }

    impl PluginManager {
        /// Create a new plugin manager
        pub fn new(plugins_dir: PathBuf, owlry_version: &str) -> Self {
            Self {
                plugins_dir,
                owlry_version: owlry_version.to_string(),
                plugins: HashMap::new(),
                disabled: Vec::new(),
            }
        }

        /// Set the list of disabled plugin IDs
        pub fn set_disabled(&mut self, disabled: Vec<String>) {
            self.disabled = disabled;
        }

        /// Discover and load all plugins from the plugins directory
        pub fn discover(&mut self) -> PluginResult<usize> {
            log::info!("Discovering plugins in {}", self.plugins_dir.display());

            let discovered = discover_plugins(&self.plugins_dir)?;
            let mut loaded_count = 0;

            for (id, (manifest, path)) in discovered {
                // Skip disabled plugins
                if self.disabled.contains(&id) {
                    log::info!("Plugin '{}' is disabled, skipping", id);
                    continue;
                }

                // Check version compatibility
                if let Err(e) = check_compatibility(&manifest, &self.owlry_version) {
                    log::warn!("Plugin '{}' is not compatible: {}", id, e);
                    continue;
                }

                let plugin = LoadedPlugin::new(manifest, path);
                self.plugins.insert(id, Rc::new(RefCell::new(plugin)));
                loaded_count += 1;
            }

            log::info!("Discovered {} compatible plugins", loaded_count);
            Ok(loaded_count)
        }

        /// Initialize all discovered plugins (load their Lua code)
        pub fn initialize_all(&mut self) -> Vec<PluginError> {
            let mut errors = Vec::new();

            for (id, plugin_rc) in &self.plugins {
                let mut plugin = plugin_rc.borrow_mut();
                if !plugin.enabled {
                    continue;
                }

                log::debug!("Initializing plugin: {}", id);
                if let Err(e) = plugin.initialize() {
                    log::error!("Failed to initialize plugin '{}': {}", id, e);
                    errors.push(e);
                    plugin.enabled = false;
                }
            }

            errors
        }

        /// Get a loaded plugin by ID (returns Rc for shared ownership)
        #[allow(dead_code)]
        pub fn get(&self, id: &str) -> Option<Rc<RefCell<LoadedPlugin>>> {
            self.plugins.get(id).cloned()
        }

        /// Get all loaded plugins
        #[allow(dead_code)]
        pub fn plugins(&self) -> impl Iterator<Item = Rc<RefCell<LoadedPlugin>>> + '_ {
            self.plugins.values().cloned()
        }

        /// Get all enabled plugins
        pub fn enabled_plugins(&self) -> impl Iterator<Item = Rc<RefCell<LoadedPlugin>>> + '_ {
            self.plugins
                .values()
                .filter(|p| p.borrow().enabled)
                .cloned()
        }

        /// Get the number of loaded plugins
        #[allow(dead_code)]
        pub fn plugin_count(&self) -> usize {
            self.plugins.len()
        }

        /// Get the number of enabled plugins
        #[allow(dead_code)]
        pub fn enabled_count(&self) -> usize {
            self.plugins.values().filter(|p| p.borrow().enabled).count()
        }

        /// Enable a plugin by ID
        #[allow(dead_code)]
        pub fn enable(&mut self, id: &str) -> PluginResult<()> {
            let plugin_rc = self
                .plugins
                .get(id)
                .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
            let mut plugin = plugin_rc.borrow_mut();

            if !plugin.enabled {
                plugin.enabled = true;
                // Initialize if not already done
                plugin.initialize()?;
            }

            Ok(())
        }

        /// Disable a plugin by ID
        #[allow(dead_code)]
        pub fn disable(&mut self, id: &str) -> PluginResult<()> {
            let plugin_rc = self
                .plugins
                .get(id)
                .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
            plugin_rc.borrow_mut().enabled = false;
            Ok(())
        }

        /// Get plugin IDs that provide a specific feature
        #[allow(dead_code)]
        pub fn providers_for(&self, provider_name: &str) -> Vec<String> {
            self.enabled_plugins()
                .filter(|p| {
                    p.borrow()
                        .manifest
                        .provides
                        .providers
                        .contains(&provider_name.to_string())
                })
                .map(|p| p.borrow().id().to_string())
                .collect()
        }

        /// Check if any plugin provides actions
        #[allow(dead_code)]
        pub fn has_action_plugins(&self) -> bool {
            self.enabled_plugins()
                .any(|p| p.borrow().manifest.provides.actions)
        }

        /// Check if any plugin provides hooks
        #[allow(dead_code)]
        pub fn has_hook_plugins(&self) -> bool {
            self.enabled_plugins()
                .any(|p| p.borrow().manifest.provides.hooks)
        }

        /// Get all theme names provided by plugins
        #[allow(dead_code)]
        pub fn theme_names(&self) -> Vec<String> {
            self.enabled_plugins()
                .flat_map(|p| p.borrow().manifest.provides.themes.clone())
                .collect()
        }

        /// Create providers from all enabled plugins
        ///
        /// This must be called after `initialize_all()`. Returns a vec of Provider trait
        /// objects that can be added to the ProviderManager.
        pub fn create_providers(&self) -> Vec<Box<dyn crate::providers::Provider>> {
            use crate::providers::lua_provider::create_providers_from_plugin;

            let mut providers = Vec::new();

            for plugin_rc in self.enabled_plugins() {
                let plugin_providers = create_providers_from_plugin(plugin_rc);
                providers.extend(plugin_providers);
            }

            providers
        }
    }
}

#[cfg(feature = "lua")]
pub use lua_manager::PluginManager;

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "lua"))]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_plugin(dir: &std::path::Path, id: &str, version: &str, owlry_req: &str) {
        let plugin_dir = dir.join(id);
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = format!(
            r#"
[plugin]
id = "{}"
name = "Test {}"
version = "{}"
owlry_version = "{}"

[provides]
providers = ["{}"]
"#,
            id, id, version, owlry_req, id
        );
        fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
        fs::write(plugin_dir.join("init.lua"), "-- test plugin").unwrap();
    }

    #[test]
    fn test_plugin_manager_discover() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "plugin-a", "1.0.0", ">=0.3.0");
        create_test_plugin(temp.path(), "plugin-b", "2.0.0", ">=0.3.0");

        let mut manager = PluginManager::new(temp.path().to_path_buf(), "0.3.5");
        let count = manager.discover().unwrap();

        assert_eq!(count, 2);
        assert!(manager.get("plugin-a").is_some());
        assert!(manager.get("plugin-b").is_some());
    }

    #[test]
    fn test_plugin_manager_disabled() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "plugin-a", "1.0.0", ">=0.3.0");
        create_test_plugin(temp.path(), "plugin-b", "1.0.0", ">=0.3.0");

        let mut manager = PluginManager::new(temp.path().to_path_buf(), "0.3.5");
        manager.set_disabled(vec!["plugin-b".to_string()]);
        let count = manager.discover().unwrap();

        assert_eq!(count, 1);
        assert!(manager.get("plugin-a").is_some());
        assert!(manager.get("plugin-b").is_none());
    }

    #[test]
    fn test_plugin_manager_version_compat() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "old-plugin", "1.0.0", ">=0.5.0"); // Requires future version
        create_test_plugin(temp.path(), "new-plugin", "1.0.0", ">=0.3.0");

        let mut manager = PluginManager::new(temp.path().to_path_buf(), "0.3.5");
        let count = manager.discover().unwrap();

        assert_eq!(count, 1);
        assert!(manager.get("old-plugin").is_none()); // Incompatible
        assert!(manager.get("new-plugin").is_some());
    }

    #[test]
    fn test_providers_for() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "my-provider", "1.0.0", ">=0.3.0");

        let mut manager = PluginManager::new(temp.path().to_path_buf(), "0.3.5");
        manager.discover().unwrap();

        let providers = manager.providers_for("my-provider");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0], "my-provider");
    }
}
