//! systemd User Services Plugin for Owlry
//!
//! Lists and controls systemd user-level services.
//! Uses `systemctl --user` commands to interact with services.
//!
//! Each service item opens a submenu with actions like:
//! - Start/Stop/Restart/Reload/Kill
//! - Enable/Disable on startup
//! - View status and journal logs

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind,
    ProviderPosition, API_VERSION,
};
use std::process::Command;

// Plugin metadata
const PLUGIN_ID: &str = "systemd";
const PLUGIN_NAME: &str = "systemd Services";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "List and control systemd user services";

// Provider metadata
const PROVIDER_ID: &str = "systemd";
const PROVIDER_NAME: &str = "User Units";
const PROVIDER_PREFIX: &str = ":uuctl";
const PROVIDER_ICON: &str = "system-run";
const PROVIDER_TYPE_ID: &str = "uuctl";

/// systemd provider state
struct SystemdState {
    items: Vec<PluginItem>,
}

impl SystemdState {
    fn new() -> Self {
        let mut state = Self { items: Vec::new() };
        state.refresh();
        state
    }

    fn refresh(&mut self) {
        self.items.clear();

        if !Self::systemctl_available() {
            return;
        }

        // List all user services (both running and available)
        let output = match Command::new("systemctl")
            .args([
                "--user",
                "list-units",
                "--type=service",
                "--all",
                "--no-legend",
                "--no-pager",
            ])
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.items = Self::parse_systemctl_output(&stdout);

        // Sort by name
        self.items.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    }

    fn systemctl_available() -> bool {
        Command::new("systemctl")
            .args(["--user", "--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_systemctl_output(output: &str) -> Vec<PluginItem> {
        let mut items = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse systemctl output - handle variable whitespace
            // Format: UNIT LOAD ACTIVE SUB DESCRIPTION...
            let mut parts = line.split_whitespace();

            let unit_name = match parts.next() {
                Some(u) => u,
                None => continue,
            };

            // Skip if not a proper service name
            if !unit_name.ends_with(".service") {
                continue;
            }

            let _load_state = parts.next().unwrap_or("");
            let active_state = parts.next().unwrap_or("");
            let sub_state = parts.next().unwrap_or("");
            let description: String = parts.collect::<Vec<_>>().join(" ");

            // Create a clean display name
            let display_name = unit_name
                .trim_end_matches(".service")
                .replace("app-", "")
                .replace("@autostart", "")
                .replace("\\x2d", "-");

            let is_active = active_state == "active";
            let status_icon = if is_active { "●" } else { "○" };

            let status_desc = if description.is_empty() {
                format!("{} {} ({})", status_icon, sub_state, active_state)
            } else {
                format!("{} {} ({})", status_icon, description, sub_state)
            };

            // Store service info in the command field as encoded data
            // Format: SUBMENU:type_id:data where data is "unit_name:is_active"
            let submenu_data = format!("SUBMENU:uuctl:{}:{}", unit_name, is_active);

            let icon = if is_active {
                "emblem-ok-symbolic"
            } else {
                "emblem-pause-symbolic"
            };

            items.push(
                PluginItem::new(
                    format!("systemd:service:{}", unit_name),
                    display_name,
                    submenu_data,
                )
                .with_description(status_desc)
                .with_icon(icon)
                .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
            );
        }

        items
    }
}

// ============================================================================
// Submenu Action Generation (exported for core to use)
// ============================================================================

/// Generate submenu actions for a given service
/// This function is called by the core when a service is selected
pub fn actions_for_service(unit_name: &str, display_name: &str, is_active: bool) -> Vec<PluginItem> {
    let mut actions = Vec::new();

    if is_active {
        actions.push(
            PluginItem::new(
                format!("systemd:restart:{}", unit_name),
                "↻ Restart",
                format!("systemctl --user restart {}", unit_name),
            )
            .with_description(format!("Restart {}", display_name))
            .with_icon("view-refresh")
            .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
        );

        actions.push(
            PluginItem::new(
                format!("systemd:stop:{}", unit_name),
                "■ Stop",
                format!("systemctl --user stop {}", unit_name),
            )
            .with_description(format!("Stop {}", display_name))
            .with_icon("process-stop")
            .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
        );

        actions.push(
            PluginItem::new(
                format!("systemd:reload:{}", unit_name),
                "⟳ Reload",
                format!("systemctl --user reload {}", unit_name),
            )
            .with_description(format!("Reload {} configuration", display_name))
            .with_icon("view-refresh")
            .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
        );

        actions.push(
            PluginItem::new(
                format!("systemd:kill:{}", unit_name),
                "✗ Kill",
                format!("systemctl --user kill {}", unit_name),
            )
            .with_description(format!("Force kill {}", display_name))
            .with_icon("edit-delete")
            .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
        );
    } else {
        actions.push(
            PluginItem::new(
                format!("systemd:start:{}", unit_name),
                "▶ Start",
                format!("systemctl --user start {}", unit_name),
            )
            .with_description(format!("Start {}", display_name))
            .with_icon("media-playback-start")
            .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
        );
    }

    // Always available actions
    actions.push(
        PluginItem::new(
            format!("systemd:status:{}", unit_name),
            "ℹ Status",
            format!("systemctl --user status {}", unit_name),
        )
        .with_description(format!("Show {} status", display_name))
        .with_icon("dialog-information")
        .with_keywords(vec!["systemd".to_string(), "service".to_string()])
        .with_terminal(true),
    );

    actions.push(
        PluginItem::new(
            format!("systemd:journal:{}", unit_name),
            "📋 Journal",
            format!("journalctl --user -u {} -f", unit_name),
        )
        .with_description(format!("Show {} logs", display_name))
        .with_icon("utilities-system-monitor")
        .with_keywords(vec!["systemd".to_string(), "service".to_string()])
        .with_terminal(true),
    );

    actions.push(
        PluginItem::new(
            format!("systemd:enable:{}", unit_name),
            "⊕ Enable",
            format!("systemctl --user enable {}", unit_name),
        )
        .with_description(format!("Enable {} on startup", display_name))
        .with_icon("emblem-default")
        .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
    );

    actions.push(
        PluginItem::new(
            format!("systemd:disable:{}", unit_name),
            "⊖ Disable",
            format!("systemctl --user disable {}", unit_name),
        )
        .with_description(format!("Disable {} on startup", display_name))
        .with_icon("emblem-unreadable")
        .with_keywords(vec!["systemd".to_string(), "service".to_string()]),
    );

    actions
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
    let state = Box::new(SystemdState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<SystemdState>
    let state = unsafe { &mut *(handle.ptr as *mut SystemdState) };

    state.refresh();
    state.items.clone().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    let query_str = query.as_str();

    // Handle submenu action requests: ?SUBMENU:unit.service:is_active
    if let Some(data) = query_str.strip_prefix("?SUBMENU:") {
        // Parse data format: "unit_name:is_active"
        let parts: Vec<&str> = data.splitn(2, ':').collect();
        if parts.len() >= 2 {
            let unit_name = parts[0];
            let is_active = parts[1] == "true";
            let display_name = unit_name
                .trim_end_matches(".service")
                .replace("app-", "")
                .replace("@autostart", "")
                .replace("\\x2d", "-");

            return actions_for_service(unit_name, &display_name, is_active).into();
        } else if !data.is_empty() {
            // Fallback: just unit name, assume not active
            let display_name = data
                .trim_end_matches(".service")
                .replace("app-", "")
                .replace("@autostart", "")
                .replace("\\x2d", "-");
            return actions_for_service(data, &display_name, false).into();
        }
    }

    // Static provider - normal queries not used
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<SystemdState>
        unsafe {
            handle.drop_as::<SystemdState>();
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
    fn test_parse_systemctl_output() {
        let output = r#"
foo.service                          loaded active running Foo Service
bar.service                          loaded inactive dead Bar Service
baz@autostart.service               loaded active running Baz App
"#;
        let items = SystemdState::parse_systemctl_output(output);
        assert_eq!(items.len(), 3);

        // Check first item
        assert_eq!(items[0].name.as_str(), "foo");
        assert!(items[0].command.as_str().contains("SUBMENU:uuctl:foo.service:true"));

        // Check second item (inactive)
        assert_eq!(items[1].name.as_str(), "bar");
        assert!(items[1].command.as_str().contains("SUBMENU:uuctl:bar.service:false"));

        // Check third item (cleaned name)
        assert_eq!(items[2].name.as_str(), "baz");
    }

    #[test]
    fn test_actions_for_active_service() {
        let actions = actions_for_service("test.service", "Test", true);

        // Active services should have restart, stop, reload, kill + common actions
        let action_ids: Vec<_> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(action_ids.contains(&"systemd:restart:test.service"));
        assert!(action_ids.contains(&"systemd:stop:test.service"));
        assert!(action_ids.contains(&"systemd:status:test.service"));
        assert!(!action_ids.contains(&"systemd:start:test.service")); // Not for active
    }

    #[test]
    fn test_actions_for_inactive_service() {
        let actions = actions_for_service("test.service", "Test", false);

        // Inactive services should have start + common actions
        let action_ids: Vec<_> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(action_ids.contains(&"systemd:start:test.service"));
        assert!(action_ids.contains(&"systemd:status:test.service"));
        assert!(!action_ids.contains(&"systemd:stop:test.service")); // Not for inactive
    }

    #[test]
    fn test_terminal_actions() {
        let actions = actions_for_service("test.service", "Test", true);

        // Status and journal should have terminal=true
        for action in &actions {
            let id = action.id.as_str();
            if id.contains(":status:") || id.contains(":journal:") {
                assert!(action.terminal, "Action {} should have terminal=true", id);
            }
        }
    }

    #[test]
    fn test_submenu_query() {
        // Test that provider_query handles ?SUBMENU: queries correctly
        let handle = ProviderHandle { ptr: std::ptr::null_mut() };

        // Query for active service
        let query = RStr::from_str("?SUBMENU:test.service:true");
        let actions = provider_query(handle, query);
        assert!(!actions.is_empty(), "Should return actions for submenu query");

        // Should have restart action for active service
        let has_restart = actions.iter().any(|a| a.id.as_str().contains(":restart:"));
        assert!(has_restart, "Active service should have restart action");

        // Query for inactive service
        let query = RStr::from_str("?SUBMENU:test.service:false");
        let actions = provider_query(handle, query);
        assert!(!actions.is_empty(), "Should return actions for submenu query");

        // Should have start action for inactive service
        let has_start = actions.iter().any(|a| a.id.as_str().contains(":start:"));
        assert!(has_start, "Inactive service should have start action");

        // Normal query should return empty
        let query = RStr::from_str("some search");
        let actions = provider_query(handle, query);
        assert!(actions.is_empty(), "Normal query should return empty");
    }
}
