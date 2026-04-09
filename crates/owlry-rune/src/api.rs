//! Owlry API bindings for Rune plugins
//!
//! This module provides the `owlry` module that Rune plugins can use.

use rune::{Any, ContextError, Module};
use std::sync::Mutex;

use owlry_plugin_api::{PluginItem, RString};

/// Provider registration info
#[derive(Debug, Clone)]
pub struct ProviderRegistration {
    pub name: String,
    pub display_name: String,
    pub type_id: String,
    pub default_icon: String,
    pub is_static: bool,
    pub prefix: Option<String>,
    pub tab_label: Option<String>,
    pub search_noun: Option<String>,
}

/// An item returned by a provider
///
/// Exposed to Rune scripts as `owlry::Item`.
#[derive(Debug, Clone, Any)]
#[rune(item = ::owlry)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub command: String,
    pub terminal: bool,
    pub keywords: Vec<String>,
}

impl Item {
    /// Constructor exposed to Rune via #[rune::function]
    #[rune::function(path = Self::new)]
    pub fn rune_new(id: String, name: String, command: String) -> Self {
        Self {
            id,
            name,
            command,
            description: None,
            icon: None,
            terminal: false,
            keywords: Vec::new(),
        }
    }

    /// Set description (builder pattern for Rune)
    #[rune::function]
    fn description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    /// Set icon (builder pattern for Rune)
    #[rune::function]
    fn icon(mut self, icon: String) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set keywords (builder pattern for Rune)
    #[rune::function]
    fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Convert to PluginItem for FFI
    pub fn to_plugin_item(&self) -> PluginItem {
        let mut item = PluginItem::new(
            RString::from(self.id.as_str()),
            RString::from(self.name.as_str()),
            RString::from(self.command.as_str()),
        );

        if let Some(ref desc) = self.description {
            item = item.with_description(desc.clone());
        }
        if let Some(ref icon) = self.icon {
            item = item.with_icon(icon.clone());
        }

        item.with_terminal(self.terminal)
            .with_keywords(self.keywords.clone())
    }
}

/// Global state for provider registrations (thread-safe)
pub static REGISTRATIONS: Mutex<Vec<ProviderRegistration>> = Mutex::new(Vec::new());

/// Create the owlry module for Rune
pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("owlry")?;

    // Register Item type with constructor and builder methods
    module.ty::<Item>()?;
    module.function_meta(Item::rune_new)?;
    module.function_meta(Item::description)?;
    module.function_meta(Item::icon)?;
    module.function_meta(Item::keywords)?;

    // Register logging functions
    module.function("log_info", log_info).build()?;
    module.function("log_debug", log_debug).build()?;
    module.function("log_warn", log_warn).build()?;
    module.function("log_error", log_error).build()?;

    Ok(module)
}

// ============================================================================
// Logging Functions
// ============================================================================

fn log_info(message: &str) {
    log::info!("[Rune] {}", message);
}

fn log_debug(message: &str) {
    log::debug!("[Rune] {}", message);
}

fn log_warn(message: &str) {
    log::warn!("[Rune] {}", message);
}

fn log_error(message: &str) {
    log::error!("[Rune] {}", message);
}

/// Get all provider registrations
pub fn get_registrations() -> Vec<ProviderRegistration> {
    REGISTRATIONS.lock().unwrap().clone()
}

/// Clear all registrations (for testing or reloading)
pub fn clear_registrations() {
    REGISTRATIONS.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_creation() {
        let item = Item {
            id: "test-1".to_string(),
            name: "Test Item".to_string(),
            description: Some("A test".to_string()),
            icon: Some("test-icon".to_string()),
            command: "echo test".to_string(),
            terminal: false,
            keywords: vec!["test".to_string()],
        };

        let plugin_item = item.to_plugin_item();
        assert_eq!(plugin_item.id.as_str(), "test-1");
        assert_eq!(plugin_item.name.as_str(), "Test Item");
    }

    #[test]
    fn test_module_creation() {
        let module = module();
        assert!(module.is_ok());
    }
}
