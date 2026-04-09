//! Plugin discovery and loading

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mlua::Lua;
use owlry_plugin_api::PluginItem;

use crate::api;
use crate::manifest::PluginManifest;
use crate::runtime::{SandboxConfig, create_lua_runtime, load_file};

/// Provider registration info from Lua
#[derive(Debug, Clone)]
pub struct ProviderRegistration {
    pub name: String,
    pub display_name: String,
    pub type_id: String,
    pub default_icon: String,
    pub prefix: Option<String>,
    pub is_dynamic: bool,
    pub tab_label: Option<String>,
    pub search_noun: Option<String>,
}

/// A loaded plugin instance
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

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("manifest", &self.manifest)
            .field("path", &self.path)
            .field("enabled", &self.enabled)
            .field("lua", &self.lua.is_some())
            .finish()
    }
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

    /// Initialize the Lua runtime and load the entry point
    pub fn initialize(&mut self) -> Result<(), String> {
        if self.lua.is_some() {
            return Ok(()); // Already initialized
        }

        let sandbox = SandboxConfig::from_permissions(&self.manifest.permissions);
        let lua = create_lua_runtime(&sandbox)
            .map_err(|e| format!("Failed to create Lua runtime: {}", e))?;

        // Register owlry APIs before loading entry point
        api::register_apis(&lua, &self.path, self.id())
            .map_err(|e| format!("Failed to register APIs: {}", e))?;

        // Load the entry point file
        let entry_path = self.path.join(&self.manifest.plugin.entry);
        if !entry_path.exists() {
            return Err(format!(
                "Entry point '{}' not found",
                self.manifest.plugin.entry
            ));
        }

        load_file(&lua, &entry_path).map_err(|e| format!("Failed to load entry point: {}", e))?;

        self.lua = Some(lua);
        Ok(())
    }

    /// Get provider registrations from this plugin
    pub fn get_provider_registrations(&self) -> Result<Vec<ProviderRegistration>, String> {
        let lua = self
            .lua
            .as_ref()
            .ok_or_else(|| "Plugin not initialized".to_string())?;

        let mut regs = api::get_provider_registrations(lua)
            .map_err(|e| format!("Failed to get registrations: {}", e))?;

        // Fall back to manifest [[providers]] declarations when the script
        // doesn't call owlry.provider.register() (new-style plugins)
        if regs.is_empty() {
            for decl in &self.manifest.providers {
                regs.push(ProviderRegistration {
                    name: decl.id.clone(),
                    display_name: decl.name.clone(),
                    type_id: decl.type_id.clone().unwrap_or_else(|| decl.id.clone()),
                    default_icon: decl
                        .icon
                        .clone()
                        .unwrap_or_else(|| "application-x-addon".to_string()),
                    prefix: decl.prefix.clone(),
                    is_dynamic: decl.provider_type == "dynamic",
                    tab_label: decl.tab_label.clone(),
                    search_noun: decl.search_noun.clone(),
                });
            }
        }

        Ok(regs)
    }

    /// Call a provider's refresh function
    pub fn call_provider_refresh(&self, provider_name: &str) -> Result<Vec<PluginItem>, String> {
        let lua = self
            .lua
            .as_ref()
            .ok_or_else(|| "Plugin not initialized".to_string())?;

        let items = api::call_refresh(lua, provider_name)
            .map_err(|e| format!("Refresh failed: {}", e))?;

        // If the API path returned nothing, try calling the global refresh()
        // function directly (new-style plugins with manifest [[providers]])
        if items.is_empty() {
            return api::call_global_refresh(lua)
                .map_err(|e| format!("Refresh failed: {}", e));
        }

        Ok(items)
    }

    /// Call a provider's query function
    pub fn call_provider_query(
        &self,
        provider_name: &str,
        query: &str,
    ) -> Result<Vec<PluginItem>, String> {
        let lua = self
            .lua
            .as_ref()
            .ok_or_else(|| "Plugin not initialized".to_string())?;

        api::call_query(lua, provider_name, query).map_err(|e| format!("Query failed: {}", e))
    }
}

/// Discover plugins in a directory
pub fn discover_plugins(
    plugins_dir: &Path,
) -> Result<HashMap<String, (PluginManifest, PathBuf)>, String> {
    let mut plugins = HashMap::new();

    if !plugins_dir.exists() {
        return Ok(plugins);
    }

    let entries = std::fs::read_dir(plugins_dir)
        .map_err(|e| format!("Failed to read plugins directory: {}", e))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.toml");
        if !manifest_path.exists() {
            continue;
        }

        match PluginManifest::load(&manifest_path) {
            Ok(manifest) => {
                // Skip plugins whose entry point is not a Lua file
                if !manifest.plugin.entry.ends_with(".lua") {
                    log::debug!(
                        "owlry-lua: Skipping non-Lua plugin at {} (entry: {})",
                        path.display(),
                        manifest.plugin.entry
                    );
                    continue;
                }
                let id = manifest.plugin.id.clone();
                if plugins.contains_key(&id) {
                    log::warn!(
                        "owlry-lua: Duplicate plugin ID '{}', skipping {}",
                        id,
                        path.display()
                    );
                    continue;
                }
                plugins.insert(id, (manifest, path));
            }
            Err(e) => {
                log::warn!(
                    "owlry-lua: Failed to load plugin at {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    Ok(plugins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_plugin(dir: &Path, id: &str) {
        let plugin_dir = dir.join(id);
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = format!(
            r#"
[plugin]
id = "{}"
name = "Test {}"
version = "1.0.0"
"#,
            id, id
        );
        fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
        fs::write(plugin_dir.join("main.lua"), "-- empty plugin").unwrap();
    }

    #[test]
    fn test_discover_plugins() {
        let temp = TempDir::new().unwrap();
        let plugins_dir = temp.path();

        create_test_plugin(plugins_dir, "test-plugin");
        create_test_plugin(plugins_dir, "another-plugin");

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
    fn test_discover_skips_non_lua_plugins() {
        let temp = TempDir::new().unwrap();
        let plugins_dir = temp.path();

        // Rune plugin — should be skipped by the Lua runtime
        let rune_dir = plugins_dir.join("rune-plugin");
        fs::create_dir_all(&rune_dir).unwrap();
        fs::write(
            rune_dir.join("plugin.toml"),
            r#"
[plugin]
id = "rune-plugin"
name = "Rune Plugin"
version = "1.0.0"
entry_point = "main.rn"

[[providers]]
id = "rune-plugin"
name = "Rune Plugin"
"#,
        )
        .unwrap();
        fs::write(rune_dir.join("main.rn"), "pub fn refresh() { [] }").unwrap();

        // Lua plugin — should be discovered
        create_test_plugin(plugins_dir, "lua-plugin");

        let plugins = discover_plugins(plugins_dir).unwrap();
        assert_eq!(plugins.len(), 1);
        assert!(plugins.contains_key("lua-plugin"));
        assert!(!plugins.contains_key("rune-plugin"));
    }

    #[test]
    fn test_manifest_provider_fallback() {
        let temp = TempDir::new().unwrap();
        let plugin_dir = temp.path().join("test-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
[plugin]
id = "test-plugin"
name = "Test Plugin"
version = "1.0.0"
entry_point = "main.lua"

[[providers]]
id = "test-plugin"
name = "Test Plugin"
type = "static"
type_id = "testplugin"
icon = "system-run"
prefix = ":tp"
"#,
        )
        .unwrap();
        // Script that does NOT call owlry.provider.register()
        fs::write(plugin_dir.join("main.lua"), "function refresh() return {} end").unwrap();

        let manifest =
            crate::manifest::PluginManifest::load(&plugin_dir.join("plugin.toml")).unwrap();
        let mut plugin = LoadedPlugin::new(manifest, plugin_dir);
        plugin.initialize().unwrap();

        let regs = plugin.get_provider_registrations().unwrap();
        assert_eq!(regs.len(), 1, "should fall back to [[providers]] declaration");
        assert_eq!(regs[0].name, "test-plugin");
        assert_eq!(regs[0].type_id, "testplugin");
        assert_eq!(regs[0].prefix.as_deref(), Some(":tp"));
        assert!(!regs[0].is_dynamic);
    }
}
