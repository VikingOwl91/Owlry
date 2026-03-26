//! Native Plugin Provider Bridge
//!
//! This module provides a bridge between native plugins (compiled .so files)
//! and the core Provider trait used by ProviderManager.
//!
//! Native plugins are loaded from `/usr/lib/owlry/plugins/` as `.so` files
//! and provide search providers via an ABI-stable interface.

use std::sync::{Arc, RwLock};

use log::debug;
use owlry_plugin_api::{
    PluginItem as ApiPluginItem, ProviderHandle, ProviderInfo, ProviderKind, ProviderPosition,
};

use super::{LaunchItem, Provider, ProviderType};
use crate::plugins::native_loader::NativePlugin;

/// A provider backed by a native plugin
///
/// This wraps a native plugin's provider and implements the core Provider trait,
/// allowing native plugins to be used seamlessly with the existing ProviderManager.
pub struct NativeProvider {
    /// The native plugin (shared reference since multiple providers may use same plugin)
    plugin: Arc<NativePlugin>,
    /// Provider metadata
    info: ProviderInfo,
    /// Handle to the provider state in the plugin
    handle: ProviderHandle,
    /// Cached items (for static providers)
    items: RwLock<Vec<LaunchItem>>,
}

impl NativeProvider {
    /// Create a new native provider
    pub fn new(plugin: Arc<NativePlugin>, info: ProviderInfo) -> Self {
        let handle = plugin.init_provider(info.id.as_str());

        Self {
            plugin,
            info,
            handle,
            items: RwLock::new(Vec::new()),
        }
    }

    /// Get the ProviderType for this native provider
    /// All native plugins return Plugin(type_id) - the core has no hardcoded plugin types
    fn get_provider_type(&self) -> ProviderType {
        ProviderType::Plugin(self.info.type_id.to_string())
    }

    /// Convert a plugin API item to a core LaunchItem
    fn convert_item(&self, item: ApiPluginItem) -> LaunchItem {
        LaunchItem {
            id: item.id.to_string(),
            name: item.name.to_string(),
            description: item.description.as_ref().map(|s| s.to_string()).into(),
            icon: item.icon.as_ref().map(|s| s.to_string()).into(),
            provider: self.get_provider_type(),
            command: item.command.to_string(),
            terminal: item.terminal,
            tags: item.keywords.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Query the provider
    ///
    /// For dynamic providers, this is called per-keystroke.
    /// For static providers, returns cached items unless query is a special command
    /// (submenu queries `?SUBMENU:` or action commands `!ACTION:`).
    pub fn query(&self, query: &str) -> Vec<LaunchItem> {
        // Special queries (submenu, actions) should always be forwarded to the plugin
        let is_special_query = query.starts_with("?SUBMENU:") || query.starts_with("!");

        if self.info.provider_type != ProviderKind::Dynamic && !is_special_query {
            return self.items.read().unwrap().clone();
        }

        let api_items = self.plugin.query_provider(self.handle, query);
        api_items
            .into_iter()
            .map(|item| self.convert_item(item))
            .collect()
    }

    /// Check if this provider has a prefix that matches the query
    #[allow(dead_code)]
    pub fn matches_prefix(&self, query: &str) -> bool {
        match self.info.prefix.as_ref().into_option() {
            Some(prefix) => query.starts_with(prefix.as_str()),
            None => false,
        }
    }

    /// Get the prefix for this provider (if any)
    #[allow(dead_code)]
    pub fn prefix(&self) -> Option<&str> {
        self.info.prefix.as_ref().map(|s| s.as_str()).into()
    }

    /// Check if this is a dynamic provider
    #[allow(dead_code)]
    pub fn is_dynamic(&self) -> bool {
        self.info.provider_type == ProviderKind::Dynamic
    }

    /// Get the provider type ID (e.g., "calc", "clipboard", "weather")
    pub fn type_id(&self) -> &str {
        self.info.type_id.as_str()
    }

    /// Check if this is a widget provider (appears at top of results)
    pub fn is_widget(&self) -> bool {
        self.info.position == ProviderPosition::Widget
    }

    /// Get the provider's priority for result ordering
    /// Higher values appear first in results
    pub fn priority(&self) -> i32 {
        self.info.priority
    }

    /// Get the provider's default icon name
    pub fn icon(&self) -> &str {
        self.info.icon.as_str()
    }

    /// Get the provider's display position as a string
    pub fn position_str(&self) -> &str {
        match self.info.position {
            ProviderPosition::Widget => "widget",
            ProviderPosition::Normal => "normal",
        }
    }

    /// Execute an action command on the provider
    /// Uses query with "!" prefix to trigger action handling in the plugin
    pub fn execute_action(&self, action: &str) {
        let action_query = format!("!{}", action);
        self.plugin.query_provider(self.handle, &action_query);
    }
}

impl Provider for NativeProvider {
    fn name(&self) -> &str {
        self.info.name.as_str()
    }

    fn provider_type(&self) -> ProviderType {
        self.get_provider_type()
    }

    fn refresh(&mut self) {
        // Only refresh static providers
        if self.info.provider_type != ProviderKind::Static {
            return;
        }

        debug!("Refreshing native provider '{}'", self.info.name.as_str());

        let api_items = self.plugin.refresh_provider(self.handle);
        let items: Vec<LaunchItem> = api_items
            .into_iter()
            .map(|item| self.convert_item(item))
            .collect();

        debug!(
            "Native provider '{}' loaded {} items",
            self.info.name.as_str(),
            items.len()
        );

        *self.items.write().unwrap() = items;
    }

    fn items(&self) -> &[LaunchItem] {
        // This is tricky with RwLock - we need to return a reference but can't
        // hold the lock across the return. We use a raw pointer approach.
        //
        // SAFETY: The items Vec is only modified during refresh() which takes
        // &mut self, so no concurrent modification can occur while this
        // reference is live.
        unsafe {
            let guard = self.items.read().unwrap();
            let ptr = guard.as_ptr();
            let len = guard.len();
            std::slice::from_raw_parts(ptr, len)
        }
    }
}

impl Drop for NativeProvider {
    fn drop(&mut self) {
        // Clean up the provider handle
        self.plugin.drop_provider(self.handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full testing requires actual .so plugins, which we'll test
    // via integration tests. Unit tests here focus on the conversion logic.

    #[test]
    fn test_provider_type_conversion() {
        // Test that type_id is correctly converted to ProviderType::Plugin
        let type_id = "calculator";
        let provider_type = ProviderType::Plugin(type_id.to_string());

        assert_eq!(format!("{}", provider_type), "calculator");
    }
}
