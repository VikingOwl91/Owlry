//! systemd user services provider.
//!
//! Lists user-level systemd services and exposes start/stop/restart/enable/
//! disable/status/journal as submenu actions. type_id stays `uuctl` for CLI
//! and config back-compat.

use super::{ItemActivation, ItemSource, LaunchItem, Provider, ProviderType};
use log::warn;
use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Upper bound on the `systemctl --user is-active` call made while building a
/// submenu. It runs on the daemon's request path, so a wedged D-Bus must not
/// block it indefinitely.
const STATE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

const TYPE_ID: &str = "uuctl";

pub struct SystemdProvider {
    items: Vec<LaunchItem>,
}

impl Default for SystemdProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `unit` is a plausible systemd service name that can be passed as a
/// single argument to systemctl/journalctl.
///
/// Unit names use ASCII letters, digits and `:_.@-`; arbitrary bytes are
/// escaped as `\xHH`. Shell metacharacters, whitespace, leading dashes and
/// path separators are rejected before any command string is built.
fn is_valid_unit_name(unit: &str) -> bool {
    if unit.is_empty()
        || unit.len() > 256
        || unit.starts_with('-')
        || unit.contains("..")
        || !unit.ends_with(".service")
    {
        return false;
    }

    let mut chars = unit.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if chars.next() != Some('x') {
                    return false;
                }
                let high = chars.next();
                let low = chars.next();
                if !matches!(high, Some(h) if h.is_ascii_hexdigit())
                    || !matches!(low, Some(l) if l.is_ascii_hexdigit())
                {
                    return false;
                }
            }
            c if c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '@' | '-') => {}
            _ => return false,
        }
    }
    true
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

            if !is_valid_unit_name(unit_name) {
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

            let submenu_data = unit_name.to_string();

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
                // Keep the legacy command during the IPC transition so older
                // clients can still open this submenu.
                command: format!("SUBMENU:{TYPE_ID}:{submenu_data}"),
                terminal: false,
                activation: ItemActivation::Submenu {
                    provider_id: TYPE_ID.to_string(),
                    data: submenu_data,
                },
                tags: vec!["systemd".to_string(), "service".to_string()],
                source: ItemSource::Core,
            });
        }

        items
    }

    /// Run `systemctl --user is-active` with a deadline.
    ///
    /// `std::process` has no timeout, and this runs on the request path, so
    /// poll `try_wait` and kill the child if it overruns.
    fn is_active_output(unit_name: &str) -> io::Result<Output> {
        let mut child = Command::new("systemctl")
            .args(["--user", "is-active", unit_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let deadline = Instant::now() + STATE_QUERY_TIMEOUT;
        loop {
            if let Some(status) = child.try_wait()? {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    pipe.read_to_end(&mut stdout)?;
                }
                if let Some(mut pipe) = child.stderr.take() {
                    pipe.read_to_end(&mut stderr)?;
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("`systemctl --user is-active {unit_name}` timed out"),
                ));
            }
            // ponytail: 20ms poll. is-active answers in single-digit ms; a
            // condvar would buy nothing at this call rate.
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn service_is_active(unit_name: &str) -> Option<bool> {
        match Self::is_active_output(unit_name) {
            Ok(output) => {
                if let Some(is_active) = classify_active_state(
                    output.status.success(),
                    &String::from_utf8_lossy(&output.stdout),
                ) {
                    return Some(is_active);
                }
                warn!(
                    "systemd provider: failed to resolve runtime state for user unit '{}': status {}, stdout '{}', stderr '{}'",
                    unit_name,
                    output.status,
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                None
            }
            Err(error) => {
                warn!(
                    "systemd provider: failed to resolve runtime state for user unit '{}': {}",
                    unit_name, error
                );
                None
            }
        }
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
            warn!(
                "systemd provider: `systemctl --user` is unavailable; user units will remain empty until `owlry refresh uuctl` succeeds"
            );
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
            Ok(o) => {
                warn!(
                    "systemd provider: `systemctl --user list-units` failed with status {}: {}",
                    o.status,
                    String::from_utf8_lossy(&o.stderr).trim()
                );
                return;
            }
            Err(error) => {
                warn!("systemd provider: failed to run `systemctl --user list-units`: {error}");
                return;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.items = Self::parse_systemctl_output(&stdout);
        self.items.sort_by_key(|i| i.name.to_lowercase());
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

    /// Submenu data is the unit name. Its runtime state is queried now instead
    /// of using the state captured when the discovery cache was refreshed.
    fn submenu_actions(&self, data: &str) -> Vec<LaunchItem> {
        let unit_name = data.trim();
        if !is_valid_unit_name(unit_name) {
            return Vec::new();
        }
        // A `None` here means the state is genuinely unknown (transitional
        // unit, or systemctl unreachable). Show the state-independent actions
        // rather than an empty menu.
        let is_active = Self::service_is_active(unit_name);
        let display = clean_display_name(unit_name);
        actions_for_service(unit_name, &display, is_active)
    }
}

fn classify_active_state(status_success: bool, stdout: &str) -> Option<bool> {
    // systemctl exits successfully for some transitional states. Only exact,
    // stable states are safe inputs for choosing Start or Stop actions.
    match (status_success, stdout.trim()) {
        (true, "active") => Some(true),
        (false, "inactive" | "failed") => Some(false),
        _ => None,
    }
}

fn clean_display_name(unit_name: &str) -> String {
    unit_name
        .trim_end_matches(".service")
        .replace("app-", "")
        .replace("@autostart", "")
        .replace("\\x2d", "-")
}

fn make_action(
    id: &str,
    name: &str,
    program: &str,
    args: &[&str],
    desc: String,
    icon: &str,
) -> LaunchItem {
    let command = format!("{} {}", program, args.join(" "));
    LaunchItem {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(desc),
        icon: Some(icon.to_string()),
        provider: ProviderType::Plugin(TYPE_ID.into()),
        command,
        terminal: false,
        activation: ItemActivation::Argv {
            program: program.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        },
        tags: vec!["systemd".to_string(), "service".to_string()],
        source: ItemSource::Core,
    }
}

/// Build the action list for a unit.
///
/// `is_active` is `None` when the runtime state could not be resolved — a
/// transitional unit (activating/deactivating/reloading) or an unreachable user
/// bus. Lifecycle actions are selected only from a stable state, so `None`
/// offers just the state-independent set (Status/Journal/Enable/Disable) built
/// below; Start, Stop, Restart, Reload and Kill all stay behind a known state.
fn actions_for_service(unit: &str, display: &str, is_active: Option<bool>) -> Vec<LaunchItem> {
    if !is_valid_unit_name(unit) {
        return Vec::new();
    }

    let mut actions = Vec::new();

    if is_active == Some(true) {
        actions.push(make_action(
            &format!("systemd:restart:{}", unit),
            "↻ Restart",
            "systemctl",
            &["--user", "restart", unit],
            format!("Restart {}", display),
            "view-refresh",
        ));
        actions.push(make_action(
            &format!("systemd:stop:{}", unit),
            "■ Stop",
            "systemctl",
            &["--user", "stop", unit],
            format!("Stop {}", display),
            "process-stop",
        ));
        actions.push(make_action(
            &format!("systemd:reload:{}", unit),
            "⟳ Reload",
            "systemctl",
            &["--user", "reload", unit],
            format!("Reload {} configuration", display),
            "view-refresh",
        ));
        actions.push(make_action(
            &format!("systemd:kill:{}", unit),
            "✗ Kill",
            "systemctl",
            &["--user", "kill", unit],
            format!("Force kill {}", display),
            "edit-delete",
        ));
    } else if is_active == Some(false) {
        actions.push(make_action(
            &format!("systemd:start:{}", unit),
            "▶ Start",
            "systemctl",
            &["--user", "start", unit],
            format!("Start {}", display),
            "media-playback-start",
        ));
    }

    // Always-available actions. Status and Journal need a terminal.
    let mut status = make_action(
        &format!("systemd:status:{}", unit),
        "ℹ Status",
        "systemctl",
        &["--user", "status", unit],
        format!("Show {} status", display),
        "dialog-information",
    );
    status.terminal = true;
    actions.push(status);

    let mut journal = make_action(
        &format!("systemd:journal:{}", unit),
        "📋 Journal",
        "journalctl",
        &["--user", "-u", unit, "-f"],
        format!("Show {} logs", display),
        "utilities-system-monitor",
    );
    journal.terminal = true;
    actions.push(journal);

    actions.push(make_action(
        &format!("systemd:enable:{}", unit),
        "⊕ Enable",
        "systemctl",
        &["--user", "enable", unit],
        format!("Enable {} on startup", display),
        "emblem-default",
    ));
    actions.push(make_action(
        &format!("systemd:disable:{}", unit),
        "⊖ Disable",
        "systemctl",
        &["--user", "disable", unit],
        format!("Disable {} on startup", display),
        "emblem-unreadable",
    ));

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_systemctl_output_extracts_services_with_unit_submenu_data() {
        let output = "
foo.service                          loaded active running Foo Service
bar.service                          loaded inactive dead Bar Service
baz@autostart.service               loaded active running Baz App
";
        let items = SystemdProvider::parse_systemctl_output(output);
        assert_eq!(items.len(), 3);

        // Cached discovery results pass only the unit; actions resolve state later.
        assert_eq!(items[0].name, "foo");
        assert_eq!(
            items[0].activation,
            ItemActivation::Submenu {
                provider_id: "uuctl".into(),
                data: "foo.service".into(),
            }
        );

        assert_eq!(items[1].name, "bar");
        assert_eq!(
            items[1].activation,
            ItemActivation::Submenu {
                provider_id: "uuctl".into(),
                data: "bar.service".into(),
            }
        );

        // Display name cleaning strips @autostart suffix.
        assert_eq!(items[2].name, "baz");
    }

    #[test]
    fn unit_name_validation_rejects_shell_syntax() {
        assert!(is_valid_unit_name("nginx.service"));
        assert!(is_valid_unit_name("foo@bar.service"));
        assert!(is_valid_unit_name("app-org.gnome.Thing@autostart.service"));
        assert!(is_valid_unit_name(r"escaped\x2dname.service"));

        for bad in [
            "foo.service; touch /tmp/pwned",
            "foo.service && touch /tmp/pwned",
            "foo.service\nother",
            "-foo.service",
            "foo/bar.service",
            "../foo.service",
            r"foo\xZZ.service",
            r"foo\0.service",
            "foo.service ",
        ] {
            assert!(!is_valid_unit_name(bad), "accepted unsafe unit {bad:?}");
        }
    }

    #[test]
    fn escaped_unit_names_survive_into_argv() {
        let unit = r"app-foo\x2dbar.service";
        let actions = actions_for_service(unit, "foo", Some(true));
        assert!(actions.iter().any(|action| matches!(
            &action.activation,
            ItemActivation::Argv { args, .. }
                if args.iter().any(|arg| arg == unit)
        )));
    }

    #[test]
    fn hostile_submenu_data_produces_no_actions() {
        let actions = actions_for_service("foo.service; touch /tmp/pwned", "foo", Some(true));
        assert!(actions.is_empty());
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
    fn actions_for_active_service_includes_restart_stop_not_start() {
        let actions = actions_for_service("nginx.service", "nginx", Some(true));
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"systemd:restart:nginx.service"));
        assert!(ids.contains(&"systemd:stop:nginx.service"));
        assert!(ids.contains(&"systemd:status:nginx.service"));
        assert!(!ids.contains(&"systemd:start:nginx.service"));
    }

    #[test]
    fn actions_for_inactive_service_includes_start_not_stop() {
        let actions = actions_for_service("nginx.service", "nginx", Some(false));
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"systemd:start:nginx.service"));
        assert!(ids.contains(&"systemd:status:nginx.service"));
        assert!(!ids.contains(&"systemd:stop:nginx.service"));
    }

    #[test]
    fn active_state_classifier_accepts_only_stable_states() {
        assert_eq!(classify_active_state(true, "active\n"), Some(true));
        assert_eq!(classify_active_state(false, "inactive\n"), Some(false));
        assert_eq!(classify_active_state(false, "failed\n"), Some(false));

        for state in [
            "activating",
            "deactivating",
            "reloading",
            "refreshing",
            "maintenance",
            "unknown",
            "",
            "Failed to connect to bus",
        ] {
            assert_eq!(
                classify_active_state(false, state),
                None,
                "state {state:?} must not select an action set"
            );
            assert_eq!(
                classify_active_state(true, state),
                None,
                "successful state {state:?} must not select an action set"
            );
        }

        assert_eq!(classify_active_state(false, "active"), None);
        assert_eq!(classify_active_state(true, "inactive"), None);
        assert_eq!(classify_active_state(true, "failed"), None);
    }

    #[test]
    fn actions_for_unknown_state_offer_no_lifecycle_actions() {
        let actions = actions_for_service("nginx.service", "nginx", None);
        let ids: Vec<&str> = actions.iter().map(|a| a.id.as_str()).collect();
        // State-independent: safe to offer without knowing what the unit is doing.
        assert!(ids.contains(&"systemd:status:nginx.service"));
        assert!(ids.contains(&"systemd:journal:nginx.service"));
        assert!(ids.contains(&"systemd:enable:nginx.service"));
        assert!(ids.contains(&"systemd:disable:nginx.service"));
        // Every lifecycle action requires a stable state to be correct.
        for denied in [
            "systemd:start:nginx.service",
            "systemd:stop:nginx.service",
            "systemd:restart:nginx.service",
            "systemd:reload:nginx.service",
            "systemd:kill:nginx.service",
        ] {
            assert!(!ids.contains(&denied), "{denied} must not be offered");
        }
    }

    #[test]
    fn submenu_actions_returns_empty_for_garbage_data() {
        let p = SystemdProvider::new();
        assert!(p.submenu_actions("").is_empty());
    }

    #[test]
    fn submenu_actions_terminal_flag_set_on_status_and_journal() {
        for action in &actions_for_service("test.service", "test", Some(true)) {
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
