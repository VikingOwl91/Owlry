//! Minimal launch path for the Slint slice.
//!
//! The GTK launch logic lives as private inherent methods on `MainWindow`
//! (`ui/main_window/launch.rs`), so it cannot be reused without refactoring
//! that module. This reimplements the same decision tree minus the GTK
//! open-with picker: dmenu passthrough, frecency recording, script-plugin
//! command allowlist, desktop-file vs shell-command execution. Documented
//! as a migration hazard in SLINT_EXPERIMENT.md (a real migration would lift
//! launch semantics into a framework-neutral module).

use crate::backend::SearchBackend;
use crate::config::Config;
use crate::providers::{ItemActivation, ItemSource, LaunchItem, ProviderType};
use log::info;
use std::process::Command;

/// Handle activation for a selected result. Returns true when the launcher
/// should close (mirrors GTK `handle_item_action`; Submenu is handled by the
/// window glue before this is called).
pub(crate) fn handle_activation(
    item: &LaunchItem,
    config: &Config,
    backend: &mut SearchBackend,
) -> bool {
    match &item.activation {
        ItemActivation::PluginAction { command } => {
            if !backend.execute_plugin_action(command) {
                let message = format!("No provider handled action '{command}'");
                log::warn!("{message}");
                crate::notify::notify("Action failed", &message);
            }
            false
        }
        ItemActivation::Submenu { .. } => false,
        ItemActivation::Launch => {
            launch_item(item, config, backend);
            true
        }
    }
}

fn launch_item(item: &LaunchItem, config: &Config, backend: &mut SearchBackend) {
    if matches!(item.provider, ProviderType::Dmenu) {
        println!("{}", item.name);
        return;
    }

    if config.providers.frecency {
        let provider_str = item.provider.to_string();
        backend.record_launch(&item.id, &provider_str);
    }

    info!("Launching: {} ({})", item.name, item.command);

    // Same untrusted-command allowlist as the GTK frontend.
    if item.source == ItemSource::ScriptPlugin {
        let cmd = &item.command;
        let allowed = cmd.is_empty()
            || cmd.starts_with("xdg-open ")
            || cmd.starts_with("wl-copy")
            || cmd.starts_with("wl-paste");
        if !allowed {
            let msg = format!(
                "Blocked untrusted script plugin command from '{}': {}",
                item.name, cmd
            );
            log::warn!("{}", msg);
            crate::notify::notify("Command blocked", &msg);
            return;
        }
    }

    if item.command.is_empty() && !matches!(item.provider, ProviderType::Application) {
        let msg = format!("Item '{}' has no command; cannot launch", item.name);
        log::warn!("{}", msg);
        crate::notify::notify("Launch failed", &msg);
        return;
    }

    let result = if let Some(path) = filesearch_open_path(item) {
        launch_file_open(&path)
    } else if matches!(item.provider, ProviderType::Application) && item.id.ends_with(".desktop") {
        launch_desktop_file(&item.id, config)
    } else {
        launch_command(&item.command, item.terminal, config)
    };

    if let Err(e) = result {
        let msg = format!("Failed to launch '{}': {}", item.name, e);
        log::error!("{}", msg);
        crate::notify::notify("Launch failed", &msg);
    }
}

fn filesearch_open_path(item: &LaunchItem) -> Option<String> {
    match &item.provider {
        ProviderType::Plugin(id) if id == "filesearch" => {
            item.id.strip_prefix("file:").map(str::to_string)
        }
        _ => None,
    }
}

/// Slice simplification: plain `xdg-open` for file results. The GTK frontend
/// resolves the default handler first so terminal handlers open in a real
/// terminal; that resolution is private to `MainWindow` and is deferred here.
fn launch_file_open(path: &str) -> std::io::Result<std::process::Child> {
    Command::new("sh")
        .arg("-c")
        .arg(format!("xdg-open {}", shell_quote(path)))
        .spawn()
}

fn launch_desktop_file(
    desktop_path: &str,
    config: &Config,
) -> std::io::Result<std::process::Child> {
    if !std::path::Path::new(desktop_path).exists() {
        let msg = format!("Desktop file not found: {}", desktop_path);
        log::error!("{}", msg);
        crate::notify::notify("Launch failed", &msg);
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, msg));
    }

    if config.general.use_uwsm {
        let uwsm_available = Command::new("which")
            .arg("uwsm")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !uwsm_available {
            let msg = "uwsm is enabled in config but not installed";
            log::error!("{}", msg);
            crate::notify::notify("Launch failed", msg);
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, msg));
        }
        Command::new("uwsm")
            .args(["app", "--", desktop_path])
            .spawn()
    } else {
        Command::new("gio").args(["launch", desktop_path]).spawn()
    }
}

fn launch_command(
    command: &str,
    terminal: bool,
    config: &Config,
) -> std::io::Result<std::process::Child> {
    let cmd = if terminal {
        let terminal_cmd = config
            .general
            .terminal_command
            .as_deref()
            .unwrap_or("xterm");
        format!("{} -e {}", terminal_cmd, command)
    } else {
        command.to_string()
    };

    let is_system_command = cmd.starts_with("playerctl ")
        || cmd.starts_with("dbus-send ")
        || cmd.starts_with("systemctl ")
        || cmd.starts_with("journalctl ")
        || cmd.contains(" | ")
        || cmd.contains(" && ")
        || cmd.contains(" || ")
        || cmd.contains(" > ")
        || cmd.contains(" < ");

    if config.general.use_uwsm && !is_system_command {
        Command::new("uwsm")
            .args(["app", "--", "sh", "-c", &cmd])
            .spawn()
    } else {
        Command::new("sh").arg("-c").arg(&cmd).spawn()
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
