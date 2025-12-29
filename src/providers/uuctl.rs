use super::{LaunchItem, Provider, ProviderType};
use log::{debug, warn};
use std::process::Command;

/// Provider for systemd user services
/// Uses systemctl --user to list and control user-level services
pub struct UuctlProvider {
    items: Vec<LaunchItem>,
}

/// Represents the state of a systemd service
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ServiceState {
    pub unit_name: String,
    pub display_name: String,
    pub description: String,
    pub active: bool,
    pub sub_state: String,
}

impl UuctlProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn systemctl_available() -> bool {
        Command::new("systemctl")
            .arg("--user")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Generate submenu actions for a given service
    pub fn actions_for_service(unit_name: &str, display_name: &str, is_active: bool) -> Vec<LaunchItem> {
        let mut actions = Vec::new();

        if is_active {
            actions.push(LaunchItem {
                id: format!("systemd:restart:{}", unit_name),
                name: "↻ Restart".to_string(),
                description: Some(format!("Restart {}", display_name)),
                icon: Some("view-refresh".to_string()),
                provider: ProviderType::Uuctl,
                command: format!("systemctl --user restart {}", unit_name),
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
            });

            actions.push(LaunchItem {
                id: format!("systemd:stop:{}", unit_name),
                name: "■ Stop".to_string(),
                description: Some(format!("Stop {}", display_name)),
                icon: Some("process-stop".to_string()),
                provider: ProviderType::Uuctl,
                command: format!("systemctl --user stop {}", unit_name),
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
            });

            actions.push(LaunchItem {
                id: format!("systemd:reload:{}", unit_name),
                name: "⟳ Reload".to_string(),
                description: Some(format!("Reload {} configuration", display_name)),
                icon: Some("view-refresh".to_string()),
                provider: ProviderType::Uuctl,
                command: format!("systemctl --user reload {}", unit_name),
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
            });

            actions.push(LaunchItem {
                id: format!("systemd:kill:{}", unit_name),
                name: "✗ Kill".to_string(),
                description: Some(format!("Force kill {}", display_name)),
                icon: Some("edit-delete".to_string()),
                provider: ProviderType::Uuctl,
                command: format!("systemctl --user kill {}", unit_name),
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
            });
        } else {
            actions.push(LaunchItem {
                id: format!("systemd:start:{}", unit_name),
                name: "▶ Start".to_string(),
                description: Some(format!("Start {}", display_name)),
                icon: Some("media-playback-start".to_string()),
                provider: ProviderType::Uuctl,
                command: format!("systemctl --user start {}", unit_name),
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
            });
        }

        // Always available actions
        actions.push(LaunchItem {
            id: format!("systemd:status:{}", unit_name),
            name: "ℹ Status".to_string(),
            description: Some(format!("Show {} status", display_name)),
            icon: Some("dialog-information".to_string()),
            provider: ProviderType::Uuctl,
            command: format!("systemctl --user status {}", unit_name),
            terminal: true,
            tags: vec!["systemd".to_string(), "service".to_string()],
        });

        actions.push(LaunchItem {
            id: format!("systemd:journal:{}", unit_name),
            name: "📋 Journal".to_string(),
            description: Some(format!("Show {} logs", display_name)),
            icon: Some("utilities-system-monitor".to_string()),
            provider: ProviderType::Uuctl,
            command: format!("journalctl --user -u {} -f", unit_name),
            terminal: true,
            tags: vec!["systemd".to_string(), "service".to_string()],
        });

        actions.push(LaunchItem {
            id: format!("systemd:enable:{}", unit_name),
            name: "⊕ Enable".to_string(),
            description: Some(format!("Enable {} on startup", display_name)),
            icon: Some("emblem-default".to_string()),
            provider: ProviderType::Uuctl,
            command: format!("systemctl --user enable {}", unit_name),
            terminal: false,
            tags: vec!["systemd".to_string(), "service".to_string()],
        });

        actions.push(LaunchItem {
            id: format!("systemd:disable:{}", unit_name),
            name: "⊖ Disable".to_string(),
            description: Some(format!("Disable {} on startup", display_name)),
            icon: Some("emblem-unreadable".to_string()),
            provider: ProviderType::Uuctl,
            command: format!("systemctl --user disable {}", unit_name),
            terminal: false,
            tags: vec!["systemd".to_string(), "service".to_string()],
        });

        actions
    }

    fn parse_systemctl_output(output: &str) -> Vec<LaunchItem> {
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
            // Format: SUBMENU:unit_name:is_active
            let submenu_data = format!("SUBMENU:{}:{}", unit_name, is_active);

            items.push(LaunchItem {
                id: format!("systemd:service:{}", unit_name),
                name: display_name,
                description: Some(status_desc),
                icon: Some(if is_active { "emblem-ok-symbolic" } else { "emblem-pause-symbolic" }.to_string()),
                provider: ProviderType::Uuctl,
                command: submenu_data, // Special marker for submenu
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
            });
        }

        items
    }

    /// Check if an item is a submenu trigger (service, not action)
    pub fn is_submenu_item(item: &LaunchItem) -> bool {
        item.provider == ProviderType::Uuctl && item.command.starts_with("SUBMENU:")
    }

    /// Parse submenu data from item command
    pub fn parse_submenu_data(item: &LaunchItem) -> Option<(String, String, bool)> {
        if !Self::is_submenu_item(item) {
            return None;
        }

        let parts: Vec<&str> = item.command.splitn(3, ':').collect();
        if parts.len() >= 3 {
            let unit_name = parts[1].to_string();
            let is_active = parts[2] == "true";
            Some((unit_name, item.name.clone(), is_active))
        } else {
            None
        }
    }
}

impl Provider for UuctlProvider {
    fn name(&self) -> &str {
        "systemd-user"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Uuctl
    }

    fn refresh(&mut self) {
        self.items.clear();

        if !Self::systemctl_available() {
            debug!("systemctl --user not available, skipping");
            return;
        }

        // List all user services (both running and available)
        let output = match Command::new("systemctl")
            .args(["--user", "list-units", "--type=service", "--all", "--no-legend", "--no-pager"])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("Failed to run systemctl --user: {}", e);
                return;
            }
        };

        if !output.status.success() {
            warn!("systemctl --user failed with status: {}", output.status);
            return;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.items = Self::parse_systemctl_output(&stdout);

        // Sort by name
        self.items.sort_by(|a, b| a.name.cmp(&b.name));

        debug!("Found {} systemd user services", self.items.len());
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}
