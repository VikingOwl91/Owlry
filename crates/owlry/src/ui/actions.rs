//! Contextual actions for search results.
//!
//! Plain Enter keeps launching the selected result. Right Arrow opens the
//! action list built here, so contextual actions do not add provider-specific
//! branches to `main_window.rs` or change its primary activation behavior.

use std::path::Path;

use crate::providers::{ItemActivation, ItemSource, LaunchItem, ProviderType};

pub fn for_item(item: &LaunchItem) -> Vec<LaunchItem> {
    if !item.activation.is_launch() || matches!(item.provider, ProviderType::Dmenu) {
        return Vec::new();
    }

    let mut actions = vec![primary_action(item)];
    if item.source != ItemSource::Core {
        return actions;
    }

    match &item.provider {
        ProviderType::Application => application_actions(item, &mut actions),
        ProviderType::Command if !item.terminal => actions.push(command_action(
            item,
            "terminal",
            "Run in Terminal",
            item.command.clone(),
            true,
            "utilities-terminal",
        )),
        ProviderType::Plugin(provider_id) if provider_id == "filesearch" => {
            file_actions(item, &mut actions)
        }
        ProviderType::Plugin(provider_id) if provider_id == "ssh" => {
            ssh_actions(item, &mut actions)
        }
        ProviderType::Command | ProviderType::Dmenu | ProviderType::Plugin(_) => {}
    }

    actions
}

fn primary_action(item: &LaunchItem) -> LaunchItem {
    let mut action = item.clone();
    action.name = match &item.provider {
        ProviderType::Application => "Open",
        ProviderType::Plugin(id) if id == "filesearch" => "Open",
        ProviderType::Plugin(id) if id == "ssh" => "Connect",
        ProviderType::Plugin(id)
            if matches!(id.as_str(), "calc" | "conv" | "clipboard" | "emoji") =>
        {
            "Copy"
        }
        ProviderType::Plugin(id) if id == "websearch" => "Open in Browser",
        _ => "Run",
    }
    .to_string();
    action.description = Some(item.name.clone());
    action
}

fn application_actions(item: &LaunchItem, actions: &mut Vec<LaunchItem>) {
    actions.push(command_action(
        item,
        "terminal",
        "Open in Terminal",
        item.command.clone(),
        true,
        "utilities-terminal",
    ));

    if let Some(parent) = Path::new(&item.id).parent() {
        actions.push(command_action(
            item,
            "desktop-location",
            "Show Desktop File Location",
            format!("xdg-open {}", shell_quote(&parent.to_string_lossy())),
            false,
            "folder",
        ));
    }
}

fn file_actions(item: &LaunchItem, actions: &mut Vec<LaunchItem>) {
    let Some(path) = item.id.strip_prefix("file:") else {
        return;
    };
    let path_obj = Path::new(path);
    let folder = if path_obj.is_dir() {
        path_obj
    } else {
        path_obj.parent().unwrap_or(path_obj)
    };

    actions.push(command_action(
        item,
        "folder",
        "Open Containing Folder",
        format!("xdg-open {}", shell_quote(&folder.to_string_lossy())),
        false,
        "folder",
    ));
    actions.push(command_action(
        item,
        "copy-path",
        "Copy Path",
        format!("printf '%s' {} | wl-copy", shell_quote(path)),
        false,
        "edit-copy",
    ));
    actions.push(command_action(
        item,
        "trash",
        "Move to Trash",
        format!("gio trash {}", shell_quote(path)),
        false,
        "user-trash",
    ));
}

fn ssh_actions(item: &LaunchItem, actions: &mut Vec<LaunchItem>) {
    if let Some(host) = item.id.strip_prefix("ssh:") {
        actions.push(command_action(
            item,
            "copy-host",
            "Copy Host Alias",
            format!("printf '%s' {} | wl-copy", shell_quote(host)),
            false,
            "edit-copy",
        ));
    }

    if let Some(config_path) = dirs::home_dir().map(|home| home.join(".ssh/config")) {
        actions.push(command_action(
            item,
            "edit-config",
            "Edit SSH Config",
            format!("xdg-open {}", shell_quote(&config_path.to_string_lossy())),
            false,
            "document-edit",
        ));
    }
}

fn command_action(
    parent: &LaunchItem,
    suffix: &str,
    name: &str,
    command: String,
    terminal: bool,
    icon: &str,
) -> LaunchItem {
    LaunchItem {
        id: format!("action:{suffix}:{}", parent.id),
        name: name.to_string(),
        description: Some(parent.name.clone()),
        icon: Some(icon.to_string()),
        provider: ProviderType::Command,
        command,
        terminal,
        activation: ItemActivation::Launch,
        tags: vec!["action".into()],
        source: ItemSource::Core,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(provider: ProviderType, id: &str, command: &str) -> LaunchItem {
        LaunchItem {
            id: id.into(),
            name: "Example".into(),
            description: None,
            icon: None,
            provider,
            command: command.into(),
            terminal: false,
            activation: ItemActivation::Launch,
            tags: Vec::new(),
            source: ItemSource::Core,
        }
    }

    #[test]
    fn file_actions_include_safe_context_operations() {
        let actions = for_item(&item(
            ProviderType::Plugin("filesearch".into()),
            "file:/tmp/report's draft.txt",
            "xdg-open x",
        ));
        let names: Vec<&str> = actions.iter().map(|action| action.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Open",
                "Open Containing Folder",
                "Copy Path",
                "Move to Trash"
            ]
        );
        assert!(actions[2].command.contains("report'\\''s draft.txt"));
        assert!(actions[3].command.starts_with("gio trash "));
    }

    #[test]
    fn application_actions_include_terminal_and_desktop_location() {
        let actions = for_item(&item(
            ProviderType::Application,
            "/usr/share/applications/demo.desktop",
            "demo",
        ));
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[1].name, "Open in Terminal");
        assert!(actions[1].terminal);
        assert_eq!(actions[2].name, "Show Desktop File Location");
    }

    #[test]
    fn every_regular_result_has_its_primary_action() {
        let actions = for_item(&item(
            ProviderType::Plugin("calc".into()),
            "calc:2+2",
            "wl-copy 4",
        ));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Copy");
    }

    #[test]
    fn script_results_do_not_gain_trusted_extra_commands() {
        let mut script = item(
            ProviderType::Plugin("custom".into()),
            "custom:1",
            "wl-copy ok",
        );
        script.source = ItemSource::ScriptPlugin;
        assert_eq!(for_item(&script).len(), 1);
    }

    #[test]
    fn provider_submenus_are_not_wrapped_in_another_action_menu() {
        let mut submenu = item(
            ProviderType::Plugin("uuctl".into()),
            "systemd:demo",
            "legacy",
        );
        submenu.activation = ItemActivation::Submenu {
            provider_id: "uuctl".into(),
            data: "demo.service:true".into(),
        };
        assert!(for_item(&submenu).is_empty());
    }
}
