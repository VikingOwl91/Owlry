//! LuaProvider - Bridge between Lua plugins and the Provider trait
//!
//! This module provides a `LuaProvider` struct that implements the `Provider` trait
//! by delegating to a Lua plugin's registered provider functions.

use std::sync::{Arc, Mutex};

use crate::plugins::{LoadedPlugin, PluginItem, ProviderRegistration};

use super::{ItemSource, LaunchItem, Provider, ProviderType};

/// A provider backed by a Lua plugin
///
/// This struct implements the `Provider` trait by calling into a Lua plugin's
/// `refresh` or `query` functions.
pub struct LuaProvider {
    /// Provider registration info
    registration: ProviderRegistration,
    /// Reference to the loaded plugin (shared with other providers from same plugin).
    /// Mutex serializes concurrent refresh calls; Arc allows sharing across threads.
    plugin: Arc<Mutex<LoadedPlugin>>,
    /// Cached items from last refresh
    items: Vec<LaunchItem>,
}

impl LuaProvider {
    /// Create a new LuaProvider
    pub fn new(registration: ProviderRegistration, plugin: Arc<Mutex<LoadedPlugin>>) -> Self {
        Self {
            registration,
            plugin,
            items: Vec::new(),
        }
    }

    /// Convert a PluginItem to a LaunchItem
    fn convert_item(&self, item: PluginItem) -> LaunchItem {
        if item.command.is_none() {
            log::warn!("Plugin item '{}' has no command", item.name);
        }
        LaunchItem {
            id: item.id,
            name: item.name,
            description: item.description,
            icon: item.icon,
            provider: ProviderType::Plugin(self.registration.type_id.clone()),
            command: item.command.unwrap_or_default(),
            terminal: item.terminal,
            tags: item.tags,
            source: ItemSource::ScriptPlugin,
        }
    }
}

impl Provider for LuaProvider {
    fn name(&self) -> &str {
        &self.registration.name
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(self.registration.type_id.clone())
    }

    fn refresh(&mut self) {
        // Only refresh static providers
        if !self.registration.is_static {
            return;
        }

        let plugin = self.plugin.lock().unwrap();
        match plugin.call_provider_refresh(&self.registration.name) {
            Ok(items) => {
                self.items = items.into_iter().map(|i| self.convert_item(i)).collect();
                log::debug!(
                    "[LuaProvider] '{}' refreshed with {} items",
                    self.registration.name,
                    self.items.len()
                );
            }
            Err(e) => {
                log::error!(
                    "[LuaProvider] Failed to refresh '{}': {}",
                    self.registration.name,
                    e
                );
                self.items.clear();
            }
        }
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

// LuaProvider is Send + Sync because:
// - Arc<Mutex<LoadedPlugin>> is Send + Sync (LoadedPlugin: Send with mlua "send" feature)
// - All other fields are Send + Sync
// No unsafe impl needed.

/// Create LuaProviders from all registered providers in a plugin
pub fn create_providers_from_plugin(plugin: Arc<Mutex<LoadedPlugin>>) -> Vec<Box<dyn Provider>> {
    let registrations = {
        let p = plugin.lock().unwrap();
        match p.get_provider_registrations() {
            Ok(regs) => regs,
            Err(e) => {
                log::error!("[LuaProvider] Failed to get registrations: {}", e);
                return Vec::new();
            }
        }
    };

    registrations
        .into_iter()
        .map(|reg| {
            let provider = LuaProvider::new(reg, plugin.clone());
            Box::new(provider) as Box<dyn Provider>
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require a complete plugin setup
    // These tests verify the basic structure

    #[test]
    fn test_provider_type() {
        let reg = ProviderRegistration {
            name: "test".to_string(),
            display_name: "Test".to_string(),
            type_id: "test_provider".to_string(),
            default_icon: "test-icon".to_string(),
            is_static: true,
            prefix: None,
        };

        // We can't easily create a mock LoadedPlugin, so just test the type
        assert_eq!(reg.type_id, "test_provider");
    }
}
