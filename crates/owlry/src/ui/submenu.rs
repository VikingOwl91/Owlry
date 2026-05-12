//! Universal Submenu Support for Plugins
//!
//! Provides parsing utilities for submenu commands. Plugins handle their own
//! submenu action generation through the query interface.
//!
//! ## Command Format
//!
//! Plugins should use this command format for submenu items:
//! ```text
//! SUBMENU:<plugin_id>:<data>
//! ```
//!
//! For example:
//! - `SUBMENU:systemd:nginx.service:true` (systemd service with active state)
//! - `SUBMENU:docker:container_id:running` (docker container with state)
//!
//! ## How It Works
//!
//! 1. Plugin returns items with `SUBMENU:...` commands
//! 2. When user selects item, main_window detects it's a submenu item
//! 3. main_window queries the plugin via `?SUBMENU:<data>` format
//! 4. Plugin returns submenu action items
//! 5. main_window displays the submenu
//!
//! ## Plugin Implementation
//!
//! Plugins should handle submenu queries in their `provider_query` function:
//!
//! ```rust,ignore
//! extern "C" fn provider_query(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
//!     let query_str = query.as_str();
//!
//!     // Handle submenu action requests
//!     if let Some(data) = query_str.strip_prefix("?SUBMENU:") {
//!         return generate_submenu_actions(data);
//!     }
//!
//!     // Handle action execution
//!     if let Some(action) = query_str.strip_prefix("!") {
//!         execute_action(action);
//!         return RVec::new();
//!     }
//!
//!     // Normal search query
//!     search(query_str)
//! }
//! ```

use owlry_core::providers::LaunchItem;

/// Parse a submenu command and extract plugin_id and data
/// Returns (plugin_id, data) if command matches SUBMENU: format
pub fn parse_submenu_command(command: &str) -> Option<(&str, &str)> {
    let rest = command.strip_prefix("SUBMENU:")?;
    let colon_pos = rest.find(':')?;
    let plugin_id = &rest[..colon_pos];
    let data = &rest[colon_pos + 1..];
    Some((plugin_id, data))
}

/// Check if an item should open a submenu
pub fn is_submenu_item(item: &LaunchItem) -> bool {
    item.command.starts_with("SUBMENU:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use owlry_core::providers::{ItemSource, ProviderType};

    #[test]
    fn test_parse_submenu_command() {
        assert_eq!(
            parse_submenu_command("SUBMENU:systemd:nginx.service:true"),
            Some(("systemd", "nginx.service:true"))
        );
        assert_eq!(
            parse_submenu_command("SUBMENU:docker:abc123:running"),
            Some(("docker", "abc123:running"))
        );
        assert_eq!(parse_submenu_command("not-a-submenu"), None);
        assert_eq!(parse_submenu_command("SUBMENU:"), None);
        assert_eq!(parse_submenu_command("SUBMENU:nocolon"), None);
    }

    #[test]
    fn test_is_submenu_item() {
        let submenu_item = LaunchItem {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: None,
            icon: None,
            provider: ProviderType::Plugin("test".to_string()),
            command: "SUBMENU:plugin:data".to_string(),
            terminal: false,
            tags: vec![],
            source: ItemSource::Core,
        };
        assert!(is_submenu_item(&submenu_item));

        let normal_item = LaunchItem {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: None,
            icon: None,
            provider: ProviderType::Plugin("test".to_string()),
            command: "some-command".to_string(),
            terminal: false,
            tags: vec![],
            source: ItemSource::Core,
        };
        assert!(!is_submenu_item(&normal_item));
    }
}
