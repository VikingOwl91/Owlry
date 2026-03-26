//! Lua plugin loading and initialization

use std::path::PathBuf;

use mlua::Lua;

use super::api;
use super::error::{PluginError, PluginResult};
use super::manifest::PluginManifest;
use super::runtime::{SandboxConfig, create_lua_runtime, load_file};

/// A loaded plugin instance
#[derive(Debug)]
pub struct LoadedPlugin {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Path to plugin directory
    pub path: PathBuf,
    /// Whether plugin is enabled
    pub enabled: bool,
    /// Lua runtime (None if not yet initialized)
    lua: Option<Lua>,
}

impl LoadedPlugin {
    /// Create a new loaded plugin (not yet initialized)
    pub fn new(manifest: PluginManifest, path: PathBuf) -> Self {
        Self {
            manifest,
            path,
            enabled: true,
            lua: None,
        }
    }

    /// Get the plugin ID
    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    /// Get the plugin name
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    /// Initialize the Lua runtime and load the entry point
    pub fn initialize(&mut self) -> PluginResult<()> {
        if self.lua.is_some() {
            return Ok(()); // Already initialized
        }

        let sandbox = SandboxConfig::from_permissions(&self.manifest.permissions);
        let lua = create_lua_runtime(&sandbox).map_err(|e| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: e.to_string(),
        })?;

        // Register owlry APIs before loading entry point
        api::register_apis(&lua, &self.path, self.id()).map_err(|e| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: format!("Failed to register APIs: {}", e),
        })?;

        // Load the entry point file
        let entry_path = self.path.join(&self.manifest.plugin.entry);
        if !entry_path.exists() {
            return Err(PluginError::InvalidManifest {
                plugin: self.id().to_string(),
                message: format!("Entry point '{}' not found", self.manifest.plugin.entry),
            });
        }

        load_file(&lua, &entry_path).map_err(|e| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: e.to_string(),
        })?;

        self.lua = Some(lua);
        Ok(())
    }

    /// Get provider registrations from this plugin
    pub fn get_provider_registrations(&self) -> PluginResult<Vec<super::ProviderRegistration>> {
        let lua = self.lua.as_ref().ok_or_else(|| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: "Plugin not initialized".to_string(),
        })?;

        api::get_provider_registrations(lua).map_err(|e| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: e.to_string(),
        })
    }

    /// Call a provider's refresh function
    pub fn call_provider_refresh(
        &self,
        provider_name: &str,
    ) -> PluginResult<Vec<super::PluginItem>> {
        let lua = self.lua.as_ref().ok_or_else(|| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: "Plugin not initialized".to_string(),
        })?;

        api::provider::call_refresh(lua, provider_name).map_err(|e| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: e.to_string(),
        })
    }

    /// Call a provider's query function
    #[allow(dead_code)] // Will be used for dynamic query providers
    pub fn call_provider_query(
        &self,
        provider_name: &str,
        query: &str,
    ) -> PluginResult<Vec<super::PluginItem>> {
        let lua = self.lua.as_ref().ok_or_else(|| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: "Plugin not initialized".to_string(),
        })?;

        api::provider::call_query(lua, provider_name, query).map_err(|e| PluginError::LuaError {
            plugin: self.id().to_string(),
            message: e.to_string(),
        })
    }

    /// Get a reference to the Lua runtime (if initialized)
    #[allow(dead_code)]
    pub fn lua(&self) -> Option<&Lua> {
        self.lua.as_ref()
    }

    /// Get a mutable reference to the Lua runtime (if initialized)
    #[allow(dead_code)]
    pub fn lua_mut(&mut self) -> Option<&mut Lua> {
        self.lua.as_mut()
    }
}

// Note: discover_plugins and check_compatibility are in manifest.rs
// to avoid Lua dependency for plugin discovery.

#[cfg(test)]
mod tests {
    use super::super::manifest::{check_compatibility, discover_plugins};
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_plugin(dir: &Path, id: &str, name: &str) {
        let plugin_dir = dir.join(id);
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = format!(
            r#"
[plugin]
id = "{}"
name = "{}"
version = "1.0.0"
"#,
            id, name
        );
        fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
        fs::write(plugin_dir.join("init.lua"), "-- empty plugin").unwrap();
    }

    #[test]
    fn test_discover_plugins() {
        let temp = TempDir::new().unwrap();
        let plugins_dir = temp.path();

        create_test_plugin(plugins_dir, "test-plugin", "Test Plugin");
        create_test_plugin(plugins_dir, "another-plugin", "Another Plugin");

        let plugins = discover_plugins(plugins_dir).unwrap();
        assert_eq!(plugins.len(), 2);
        assert!(plugins.contains_key("test-plugin"));
        assert!(plugins.contains_key("another-plugin"));
    }

    #[test]
    fn test_discover_plugins_empty_dir() {
        let temp = TempDir::new().unwrap();
        let plugins = discover_plugins(temp.path()).unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_discover_plugins_nonexistent_dir() {
        let plugins = discover_plugins(Path::new("/nonexistent/path")).unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_check_compatibility() {
        let toml_str = r#"
[plugin]
id = "test"
name = "Test"
version = "1.0.0"
owlry_version = ">=0.3.0"
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();

        assert!(check_compatibility(&manifest, "0.3.5").is_ok());
        assert!(check_compatibility(&manifest, "0.2.0").is_err());
    }
}
