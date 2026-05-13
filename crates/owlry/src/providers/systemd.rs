//! systemd user services provider.
//!
//! Lists user-level systemd services and exposes start/stop/restart/enable/
//! disable/status/journal as submenu actions. type_id stays `uuctl` for CLI
//! and config back-compat.

use super::{ItemSource, LaunchItem, Provider, ProviderType};
use std::process::Command;

const TYPE_ID: &str = "uuctl";

pub struct SystemdProvider {
    items: Vec<LaunchItem>,
}

impl Default for SystemdProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemdProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn systemctl_available() -> bool {
        Command::new("systemctl")
            .args(["--user", "--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_systemctl_output(output: &str) -> Vec<LaunchItem> {
        let mut items = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let mut parts = line.split_whitespace();

            let unit_name = match parts.next() {
                Some(u) => u,
                None => continue,
            };

            if !unit_name.ends_with(".service") {
                continue;
            }

            let _load_state = parts.next().unwrap_or("");
            let active_state = parts.next().unwrap_or("");
            let sub_state = parts.next().unwrap_or("");
            let description: String = parts.collect::<Vec<_>>().join(" ");

            let display_name = clean_display_name(unit_name);

            let is_active = active_state == "active";
            let status_icon = if is_active { "●" } else { "○" };

            let status_desc = if description.is_empty() {
                format!("{} {} ({})", status_icon, sub_state, active_state)
            } else {
                format!("{} {} ({})", status_icon, description, sub_state)
            };

            // SUBMENU:<type_id>:<data> is the protocol the UI uses to route
            // a selection back to this provider's submenu_actions().
            let submenu_data = format!("SUBMENU:{}:{}:{}", TYPE_ID, unit_name, is_active);

            let icon = if is_active {
                "emblem-ok-symbolic"
            } else {
                "emblem-pause-symbolic"
            };

            items.push(LaunchItem {
                id: format!("systemd:service:{}", unit_name),
                name: display_name,
                description: Some(status_desc),
                icon: Some(icon.to_string()),
                provider: ProviderType::Plugin(TYPE_ID.into()),
                command: submenu_data,
                terminal: false,
                tags: vec!["systemd".to_string(), "service".to_string()],
                source: ItemSource::Core,
            });
        }

        items
    }
}

impl Provider for SystemdProvider {
    fn name(&self) -> &str {
        "User Units"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        self.items.clear();

        if !Self::systemctl_available() {
            return;
        }

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
        self.items
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn prefix(&self) -> Option<&str> {
        Some(":uuctl")
    }

    fn icon(&self) -> &str {
        "system-run"
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Units")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("systemd units")
    }

    /// Submenu data is `<unit_name>:<is_active>` (encoded by `refresh()`).
    fn submenu_actions(&self, data: &str) -> Vec<LaunchItem> {
        let parts: Vec<&str> = data.splitn(2, ':').collect();
        let (unit_name, is_active) = match parts.as_slice() {
            [unit, active] if !unit.is_empty() => (*unit, *active == "true"),
            [unit] if !unit.is_empty() => (*unit, false),
            _ => return Vec::new(),
        };
        let display = clean_display_name(unit_name);
        actions_for_service(unit_name, &display, is_active)
    }
}

fn clean_display_name(unit_name: &str) -> String {
    unit_name
        .trim_end_matches(".service")
        .replace("app-", "")
        .replace("@autostart", "")
        .replace("\\x2d", "-")
}

fn make_action(id: &str, name: &str, command: String, desc: String, icon: &str) -> LaunchItem {
    LaunchItem {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(desc),
        icon: Some(icon.to_string()),
        provider: ProviderType::Plugin(TYPE_ID.into()),
        command,
        terminal: false,
        tags: vec!["systemd".to_string(), "service".to_string()],
        source: ItemSource::Core,
    }
}

fn actions_for_service(unit: &str, display: &str, is_active: bool) -> Vec<LaunchItem> {
    let mut actions = Vec::new();

    if is_active {
        actions.push(make_action(
            &format!("systemd:restart:{}", unit),
            "↻ Restart",
            format!("systemctl --user restart {}", unit),
            format!("Restart {}", display),
            "view-refresh",
        ));
        actions.push(make_action(
            &format!("systemd:stop:{}", unit),
            "■ Stop",
            format!("systemctl --user stop {}", unit),
            format!("Stop {}", display),
            "process-stop",
        ));
        actions.push(make_action(
            &format!("systemd:reload:{}", unit),
            "⟳ Reload",
            format!("systemctl --user reload {}", unit),
            format!("Reload {} configuration", display),
            "view-refresh",
        ));
        actions.push(make_action(
            &format!("systemd:kill:{}", unit),
            "✗ Kill",
            format!("systemctl --user kill {}", unit),
            format!("Force kill {}", display),
            "edit-delete",
        ));
    } else {
        actions.push(make_action(
            &format!("systemd:start:{}", unit),
            "▶ Start",
            format!("systemctl --user start {}", unit),
            format!("Start {}", display),
            "media-playback-start",
        ));
    }

    // Always-available actions. Status and Journal need a terminal.
    let mut status = make_action(
        &format!("systemd:status:{}", unit),
        "ℹ Status",
        format!("systemctl --user status {}", unit),
        format!("Show {} status", display),
        "dialog-information",
    );
    status.terminal = true;
    actions.push(status);

    let mut journal = make_action(
        &format!("systemd:journal:{}", unit),
        "📋 Journal",
        format!("journalctl --user -u {} -f", unit),
        format!("Show {} logs", display),
        "utilities-system-monitor",
    );
    journal.terminal = true;
    actions.push(journal);

    actions.push(make_action(
        &format!("systemd:enable:{}", unit),
        "⊕ Enable",
        format!("systemctl --user enable {}", unit),
        format!("Enable {} on startup", display),
        "emblem-default",
    ));
    actions.push(make_action(
        &format!("systemd:disable:{}", unit),
        "⊖ Disable",
        format!("systemctl --user disable {}", unit),
        format!("Disable {} on startup", display),
        "emblem-unreadable",
    ));

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_systemctl_output_extracts_services_with_submenu_command() {
        let output = "
foo.service                          loaded active running Foo Service
bar.service                          loaded inactive dead Bar Service
baz@autostart.service               loaded active running Baz App
";
        let items = SystemdProvider::parse_systemctl_output(output);
        assert_eq!(items.len(), 3);

        // Active service: SUBMENU command with is_active=true.
        assert_eq!(items[0].name, "foo");
        assert!(items[0].command.contains("SUBMENU:uuctl:foo.service:true"));

        // Inactive service: SUBMENU command with is_active=false.
        assert_eq!(items[1].name, "bar");
        assert!(items[1].command.contains("SUBMENU:uuctl:bar.service:false"));

        // Display name cleaning strips @autostart suffix.
        assert_eq!(items[2].name, "baz");
    }

    #[test]
    fn parse_systemctl_output_skips_non_service_lines() {
        let output = "
foo.service       loaded active running Foo
bar.socket        loaded active listening Bar
quux.timer        loaded active waiting Quux
baz.service       loaded inactive dead Baz
";
        let items = SystemdProvider::parse_systemctl_output(output);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "foo");
        assert_eq!(items[1].name, "baz");
    }

    #[test]
    fn provider_type_is_uuctl_plugin() {
        // CLI back-compat: `-m uuctl` and `:uuctl` both resolve to this type_id.
        let p = SystemdProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("uuctl".into()));
    }

    #[test]
    fn submenu_actions_for_active_service_includes_restart_stop_not_start() {
        let p = SystemdProvider::new();
        let actions = p.submenu_actions("nginx.service:true");
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"systemd:restart:nginx.service"));
        assert!(ids.contains(&"systemd:stop:nginx.service"));
        assert!(ids.contains(&"systemd:status:nginx.service"));
        assert!(!ids.contains(&"systemd:start:nginx.service"));
    }

    #[test]
    fn submenu_actions_for_inactive_service_includes_start_not_stop() {
        let p = SystemdProvider::new();
        let actions = p.submenu_actions("nginx.service:false");
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"systemd:start:nginx.service"));
        assert!(ids.contains(&"systemd:status:nginx.service"));
        assert!(!ids.contains(&"systemd:stop:nginx.service"));
    }

    #[test]
    fn submenu_actions_returns_empty_for_garbage_data() {
        let p = SystemdProvider::new();
        assert!(p.submenu_actions("").is_empty());
    }

    #[test]
    fn submenu_actions_terminal_flag_set_on_status_and_journal() {
        let p = SystemdProvider::new();
        let actions = p.submenu_actions("test.service:true");
        for action in &actions {
            if action.id.contains(":status:") || action.id.contains(":journal:") {
                assert!(action.terminal, "{} must have terminal=true", action.id);
            }
        }
    }

    #[test]
    fn clean_display_name_strips_service_suffix_and_known_prefixes() {
        assert_eq!(clean_display_name("foo.service"), "foo");
        assert_eq!(clean_display_name("app-firefox.service"), "firefox");
        assert_eq!(clean_display_name("bar@autostart.service"), "bar");
        // The hex-escaped dash that systemd uses for some unit names.
        assert_eq!(clean_display_name("foo\\x2dbar.service"), "foo-bar");
    }
}
