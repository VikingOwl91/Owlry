//! SSH Plugin for Owlry
//!
//! A static provider that parses ~/.ssh/config and provides quick-connect
//! entries for SSH hosts.
//!
//! Examples:
//! - `SSH: myserver` → Connect to myserver
//! - `SSH: work-box` → Connect to work-box with configured user/port

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind,
    ProviderPosition, API_VERSION,
};
use std::fs;
use std::path::PathBuf;

// Plugin metadata
const PLUGIN_ID: &str = "ssh";
const PLUGIN_NAME: &str = "SSH";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Quick connect to SSH hosts from ~/.ssh/config";

// Provider metadata
const PROVIDER_ID: &str = "ssh";
const PROVIDER_NAME: &str = "SSH";
const PROVIDER_PREFIX: &str = ":ssh";
const PROVIDER_ICON: &str = "utilities-terminal";
const PROVIDER_TYPE_ID: &str = "ssh";

// Default terminal command (TODO: make configurable via plugin config)
const DEFAULT_TERMINAL: &str = "kitty";

/// SSH provider state - holds cached items
struct SshState {
    items: Vec<PluginItem>,
    terminal_command: String,
}

impl SshState {
    fn new() -> Self {
        // Try to detect terminal from environment, fall back to default
        let terminal = std::env::var("TERMINAL")
            .unwrap_or_else(|_| DEFAULT_TERMINAL.to_string());

        Self {
            items: Vec::new(),
            terminal_command: terminal,
        }
    }

    fn ssh_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".ssh").join("config"))
    }

    fn parse_ssh_config(&mut self) {
        self.items.clear();

        let config_path = match Self::ssh_config_path() {
            Some(p) => p,
            None => return,
        };

        if !config_path.exists() {
            return;
        }

        let content = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut current_host: Option<String> = None;
        let mut current_hostname: Option<String> = None;
        let mut current_user: Option<String> = None;
        let mut current_port: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Split on whitespace or '='
            let parts: Vec<&str> = line
                .splitn(2, |c: char| c.is_whitespace() || c == '=')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if parts.len() < 2 {
                continue;
            }

            let key = parts[0].to_lowercase();
            let value = parts[1];

            match key.as_str() {
                "host" => {
                    // Save previous host if exists
                    if let Some(host) = current_host.take() {
                        self.add_host_item(
                            &host,
                            current_hostname.take(),
                            current_user.take(),
                            current_port.take(),
                        );
                    }

                    // Skip wildcards and patterns
                    if !value.contains('*') && !value.contains('?') && value != "*" {
                        current_host = Some(value.to_string());
                    }
                    current_hostname = None;
                    current_user = None;
                    current_port = None;
                }
                "hostname" => {
                    current_hostname = Some(value.to_string());
                }
                "user" => {
                    current_user = Some(value.to_string());
                }
                "port" => {
                    current_port = Some(value.to_string());
                }
                _ => {}
            }
        }

        // Don't forget the last host
        if let Some(host) = current_host.take() {
            self.add_host_item(&host, current_hostname, current_user, current_port);
        }
    }

    fn add_host_item(
        &mut self,
        host: &str,
        hostname: Option<String>,
        user: Option<String>,
        port: Option<String>,
    ) {
        // Build description
        let mut desc_parts = Vec::new();
        if let Some(ref h) = hostname {
            desc_parts.push(h.clone());
        }
        if let Some(ref u) = user {
            desc_parts.push(format!("user: {}", u));
        }
        if let Some(ref p) = port {
            desc_parts.push(format!("port: {}", p));
        }

        let description = if desc_parts.is_empty() {
            None
        } else {
            Some(desc_parts.join(", "))
        };

        // Build SSH command - just use the host alias, SSH will resolve the rest
        let ssh_command = format!("ssh {}", host);

        // Wrap in terminal
        let command = format!("{} -e {}", self.terminal_command, ssh_command);

        let mut item = PluginItem::new(
            format!("ssh:{}", host),
            format!("SSH: {}", host),
            command,
        )
        .with_icon(PROVIDER_ICON)
        .with_keywords(vec!["ssh".to_string(), "remote".to_string()]);

        if let Some(desc) = description {
            item = item.with_description(desc);
        }

        self.items.push(item);
    }
}

// ============================================================================
// Plugin Interface Implementation
// ============================================================================

extern "C" fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: RString::from(PLUGIN_ID),
        name: RString::from(PLUGIN_NAME),
        version: RString::from(PLUGIN_VERSION),
        description: RString::from(PLUGIN_DESCRIPTION),
        api_version: API_VERSION,
    }
}

extern "C" fn plugin_providers() -> RVec<ProviderInfo> {
    vec![ProviderInfo {
        id: RString::from(PROVIDER_ID),
        name: RString::from(PROVIDER_NAME),
        prefix: ROption::RSome(RString::from(PROVIDER_PREFIX)),
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Static,
        type_id: RString::from(PROVIDER_TYPE_ID),
        position: ProviderPosition::Normal,
        priority: 0, // Static: use frecency ordering
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(SshState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<SshState>
    let state = unsafe { &mut *(handle.ptr as *mut SshState) };

    // Parse SSH config
    state.parse_ssh_config();

    // Return items
    state.items.to_vec().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query is handled by the core using cached items
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<SshState>
        unsafe {
            handle.drop_as::<SshState>();
        }
    }
}

// Register the plugin vtable
owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_state_new() {
        let state = SshState::new();
        assert!(state.items.is_empty());
    }

    #[test]
    fn test_parse_simple_config() {
        let mut state = SshState::new();

        // We can't easily test the full flow without mocking file paths,
        // but we can test the add_host_item method
        state.add_host_item(
            "myserver",
            Some("192.168.1.100".to_string()),
            Some("admin".to_string()),
            Some("2222".to_string()),
        );

        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].name.as_str(), "SSH: myserver");
        assert!(state.items[0].command.as_str().contains("ssh myserver"));
    }

    #[test]
    fn test_add_host_without_details() {
        let mut state = SshState::new();
        state.add_host_item("simple-host", None, None, None);

        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].name.as_str(), "SSH: simple-host");
        assert!(state.items[0].description.is_none());
    }

    #[test]
    fn test_add_host_with_partial_details() {
        let mut state = SshState::new();
        state.add_host_item("partial", Some("example.com".to_string()), None, None);

        assert_eq!(state.items.len(), 1);
        let desc = state.items[0].description.as_ref().unwrap();
        assert_eq!(desc.as_str(), "example.com");
    }

    #[test]
    fn test_items_have_icons() {
        let mut state = SshState::new();
        state.add_host_item("test", None, None, None);

        assert!(state.items[0].icon.is_some());
        assert_eq!(state.items[0].icon.as_ref().unwrap().as_str(), PROVIDER_ICON);
    }

    #[test]
    fn test_items_have_keywords() {
        let mut state = SshState::new();
        state.add_host_item("test", None, None, None);

        assert!(!state.items[0].keywords.is_empty());
        let keywords: Vec<&str> = state.items[0].keywords.iter().map(|s| s.as_str()).collect();
        assert!(keywords.contains(&"ssh"));
    }
}
