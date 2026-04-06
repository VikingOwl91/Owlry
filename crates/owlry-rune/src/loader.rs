//! Rune plugin discovery and loading

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rune::{Context, Unit};

use crate::api::{self, ProviderRegistration};
use crate::manifest::PluginManifest;
use crate::runtime::{SandboxConfig, compile_source, create_context, create_vm};

use owlry_plugin_api::PluginItem;

/// A loaded Rune plugin
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    /// Context for creating new VMs (reserved for refresh/query implementation)
    #[allow(dead_code)]
    context: Context,
    /// Compiled unit (reserved for refresh/query implementation)
    #[allow(dead_code)]
    unit: Arc<Unit>,
    registrations: Vec<ProviderRegistration>,
}

impl LoadedPlugin {
    /// Create and initialize a new plugin
    pub fn new(manifest: PluginManifest, path: PathBuf) -> Result<Self, String> {
        let sandbox = SandboxConfig::from_permissions(&manifest.permissions);
        let context =
            create_context(&sandbox).map_err(|e| format!("Failed to create context: {}", e))?;

        let entry_path = path.join(&manifest.plugin.entry);
        if !entry_path.exists() {
            return Err(format!("Entry point not found: {}", entry_path.display()));
        }

        // Clear previous registrations before loading
        api::clear_registrations();

        // Compile the source
        let unit = compile_source(&context, &entry_path)
            .map_err(|e| format!("Failed to compile: {}", e))?;

        // Run the entry point to register providers
        let mut vm =
            create_vm(&context, unit.clone()).map_err(|e| format!("Failed to create VM: {}", e))?;

        // Execute the main function if it exists
        match vm.call(rune::Hash::type_hash(["main"]), ()) {
            Ok(result) => {
                // Try to complete the execution
                let _: () = rune::from_value(result).unwrap_or(());
            }
            Err(_) => {
                // No main function is okay
            }
        }

        // Collect registrations — from runtime API or from manifest [[providers]]
        let mut registrations = api::get_registrations();
        if registrations.is_empty() && !manifest.providers.is_empty() {
            for decl in &manifest.providers {
                registrations.push(ProviderRegistration {
                    name: decl.id.clone(),
                    display_name: decl.name.clone(),
                    type_id: decl.type_id.clone().unwrap_or_else(|| decl.id.clone()),
                    default_icon: decl.icon.clone().unwrap_or_else(|| "application-x-addon".to_string()),
                    is_static: decl.provider_type != "dynamic",
                    prefix: decl.prefix.clone(),
                });
            }
        }

        log::info!(
            "Loaded Rune plugin '{}' with {} provider(s)",
            manifest.plugin.id,
            registrations.len()
        );

        Ok(Self {
            manifest,
            path,
            context,
            unit,
            registrations,
        })
    }

    /// Get plugin ID
    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    /// Get provider registrations
    pub fn provider_registrations(&self) -> &[ProviderRegistration] {
        &self.registrations
    }

    /// Check if this plugin provides a specific provider
    pub fn provides_provider(&self, name: &str) -> bool {
        self.registrations.iter().any(|r| r.name == name)
    }

    /// Refresh a static provider by calling the Rune `refresh()` function
    pub fn refresh_provider(&mut self, _name: &str) -> Result<Vec<PluginItem>, String> {
        let mut vm = create_vm(&self.context, self.unit.clone())
            .map_err(|e| format!("Failed to create VM: {}", e))?;

        let output = vm
            .call(rune::Hash::type_hash(["refresh"]), ())
            .map_err(|e| format!("refresh() call failed: {}", e))?;

        let items: Vec<crate::api::Item> = rune::from_value(output)
            .map_err(|e| format!("Failed to parse refresh() result: {}", e))?;

        Ok(items.iter().map(|i| i.to_plugin_item()).collect())
    }

    /// Query a dynamic provider by calling the Rune `query(q)` function
    pub fn query_provider(&mut self, _name: &str, query: &str) -> Result<Vec<PluginItem>, String> {
        let mut vm = create_vm(&self.context, self.unit.clone())
            .map_err(|e| format!("Failed to create VM: {}", e))?;

        let output = vm
            .call(
                rune::Hash::type_hash(["query"]),
                (query.to_string(),),
            )
            .map_err(|e| format!("query() call failed: {}", e))?;

        let items: Vec<crate::api::Item> = rune::from_value(output)
            .map_err(|e| format!("Failed to parse query() result: {}", e))?;

        Ok(items.iter().map(|i| i.to_plugin_item()).collect())
    }
}

/// Discover Rune plugins in a directory
pub fn discover_rune_plugins(plugins_dir: &Path) -> Result<HashMap<String, LoadedPlugin>, String> {
    let mut plugins = HashMap::new();

    if !plugins_dir.exists() {
        log::debug!(
            "Plugins directory does not exist: {}",
            plugins_dir.display()
        );
        return Ok(plugins);
    }

    let entries = std::fs::read_dir(plugins_dir)
        .map_err(|e| format!("Failed to read plugins directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.toml");
        if !manifest_path.exists() {
            continue;
        }

        // Load manifest
        let manifest = match PluginManifest::load(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    "Failed to load manifest at {}: {}",
                    manifest_path.display(),
                    e
                );
                continue;
            }
        };

        // Check if this is a Rune plugin (entry ends with .rn)
        if !manifest.plugin.entry.ends_with(".rn") {
            log::debug!("Skipping non-Rune plugin: {}", manifest.plugin.id);
            continue;
        }

        // Load the plugin
        match LoadedPlugin::new(manifest.clone(), path.clone()) {
            Ok(plugin) => {
                let id = manifest.plugin.id.clone();
                plugins.insert(id, plugin);
            }
            Err(e) => {
                log::warn!("Failed to load plugin '{}': {}", manifest.plugin.id, e);
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

    #[test]
    fn test_discover_empty_dir() {
        let temp = TempDir::new().unwrap();
        let plugins = discover_rune_plugins(temp.path()).unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_discover_skips_non_rune_plugins() {
        let temp = TempDir::new().unwrap();
        let plugins_dir = temp.path();

        // Lua plugin — should be skipped by the Rune runtime
        let lua_dir = plugins_dir.join("lua-plugin");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("plugin.toml"),
            r#"
[plugin]
id = "lua-plugin"
name = "Lua Plugin"
version = "1.0.0"
entry_point = "main.lua"

[[providers]]
id = "lua-plugin"
name = "Lua Plugin"
"#,
        )
        .unwrap();
        fs::write(lua_dir.join("main.lua"), "function refresh() return {} end").unwrap();

        let plugins = discover_rune_plugins(plugins_dir).unwrap();
        assert!(plugins.is_empty(), "Lua plugin should be skipped by Rune runtime");
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
entry_point = "main.rn"

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
        // Script that exports refresh() but doesn't call register_provider()
        fs::write(
            plugin_dir.join("main.rn"),
            r#"use owlry::Item;
pub fn refresh() {
    []
}
"#,
        )
        .unwrap();

        let manifest =
            crate::manifest::PluginManifest::load(&plugin_dir.join("plugin.toml")).unwrap();
        let plugin = LoadedPlugin::new(manifest, plugin_dir).unwrap();

        let regs = plugin.provider_registrations();
        assert_eq!(regs.len(), 1, "should fall back to [[providers]] declaration");
        assert_eq!(regs[0].name, "test-plugin");
        assert_eq!(regs[0].type_id, "testplugin");
        assert_eq!(regs[0].prefix.as_deref(), Some(":tp"));
        assert!(regs[0].is_static);
    }
}
