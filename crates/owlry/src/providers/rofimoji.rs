//! Rofimoji character picker integration.
//!
//! The picker itself owns the character data, fuzzy matching, frecency and
//! input insertion. Owlry exposes convenient launch entries for the common
//! character sets and for the selector/typer backends installed on the host.

use super::{ItemSource, LaunchItem, Provider, ProviderType};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const TYPE_ID: &str = "rofimoji";

/// Selector names accepted by the installed rofimoji CLI (6.7).
const SELECTORS: &[(&str, &str)] = &[
    ("rofi", "rofi"),
    ("wofi", "wofi"),
    ("fuzzel", "fuzzel"),
    ("dmenu", "dmenu"),
    ("tofi", "tofi"),
    ("bemenu", "bemenu"),
    ("wmenu", "wmenu"),
    ("choose", "choose"),
];

/// Typer names accepted by rofimoji. `wl-ime-type` is supported by current
/// rofimoji releases even though it is not always included in the automatic
/// typer preference list.
const TYPERS: &[(&str, &str)] = &[
    ("xdotool", "xdotool"),
    ("wtype", "wtype"),
    ("ydotool", "ydotool"),
    ("cliclick", "cliclick"),
    ("wl-ime-type", "wl-ime-type"),
];

pub struct RofimojiProvider {
    items: Vec<LaunchItem>,
}

impl Default for RofimojiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl RofimojiProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn executable_exists(name: &str) -> bool {
        std::env::var_os("PATH")
            .unwrap_or_default()
            .to_string_lossy()
            .split(':')
            .map(Path::new)
            .map(|dir| dir.join(name))
            .any(|candidate| {
                candidate.is_file()
                    && candidate
                        .metadata()
                        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                        .unwrap_or(false)
            })
    }

    fn command(
        files: &[&str],
        action: &str,
        selector: Option<&str>,
        typer: Option<&str>,
    ) -> String {
        let mut args = vec!["rofimoji", "--files"];
        args.extend(files.iter().copied());
        args.extend(["--action", action]);
        if let Some(selector) = selector {
            args.extend(["--selector", selector]);
        }
        if let Some(typer) = typer {
            args.extend(["--typer", typer]);
        }
        args.join(" ")
    }

    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    fn clipboard_command(character: &str) -> Option<String> {
        let quoted = Self::shell_quote(character);
        if Self::executable_exists("wl-copy") {
            Some(format!("printf '%s' {quoted} | wl-copy"))
        } else if Self::executable_exists("xclip") {
            Some(format!("printf '%s' {quoted} | xclip -selection clipboard"))
        } else if Self::executable_exists("xsel") {
            Some(format!("printf '%s' {quoted} | xsel --clipboard --input"))
        } else {
            None
        }
    }

    fn rofimoji_data_file() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("ROFIMOJI_DATA") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Some(path);
            }
        }

        // The upstream package keeps its data next to the Python `picker`
        // module. Ask the interpreter that owns the installed package instead
        // of hard-coding a Python minor version or distro path.
        let output = Command::new("python3")
            .args([
                "-c",
                "from pathlib import Path; import picker; print(Path(picker.__file__).resolve().parent / 'data' / 'kaomoji.csv')",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        path.is_file().then_some(path)
    }

    fn kaomoji_items() -> Vec<LaunchItem> {
        let Some(path) = Self::rofimoji_data_file() else {
            return Vec::new();
        };
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Vec::new();
        };

        contents
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let (character, description) = line.split_once(' ')?;
                let character = character.trim();
                let mut description = description.trim().replace('\u{feff}', "");
                if character.is_empty() || description.is_empty() {
                    return None;
                }

                // Rofimoji calls the canonical shrug entry `whatever`; keep
                // that upstream label and add the search term users expect.
                if description.split(',').any(|tag| tag.trim() == "whatever") {
                    description.push_str(", shrug");
                }

                let command = Self::clipboard_command(character)?;
                let tags: Vec<String> = description
                    .split(',')
                    .map(|tag| tag.trim().to_string())
                    .chain(["kaomoji".to_string(), "characters".to_string()])
                    .collect();

                let item = Self::item(
                    &format!("kaomoji-{index}"),
                    character,
                    &format!("Kaomoji · {description}"),
                    command,
                    &tags.iter().map(String::as_str).collect::<Vec<_>>(),
                );
                // Keep the kaomoji in the result name/description only. A
                // second, oversized text icon would duplicate the glyph.
                Some(item)
            })
            .collect()
    }

    fn item(id: &str, name: &str, description: &str, command: String, tags: &[&str]) -> LaunchItem {
        LaunchItem {
            id: format!("{TYPE_ID}:{id}"),
            name: name.to_string(),
            description: Some(description.to_string()),
            // `text:` is rendered as an intentionally blank text icon. This
            // keeps launcher rows aligned without a misleading generic icon.
            icon: Some("text:".to_string()),
            provider: ProviderType::Plugin(TYPE_ID.into()),
            command,
            terminal: false,
            activation: super::ItemActivation::Launch,
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            source: ItemSource::Core,
        }
    }

    fn build_items() -> Vec<LaunchItem> {
        if !Self::executable_exists("rofimoji") {
            return Vec::new();
        }

        let mut items = vec![
            Self::item(
                "all",
                "Emoji + Kaomoji",
                "Open rofimoji with emojis and kaomoji",
                Self::command(&["emojis", "kaomoji"], "type", None, None),
                &["emoji", "kaomoji", "characters"],
            ),
            Self::item(
                "emoji",
                "Emoji picker",
                "Open rofimoji with emojis",
                Self::command(&["emojis"], "type", None, None),
                &["emoji", "characters"],
            ),
            Self::item(
                "kaomoji",
                "Kaomoji picker",
                "Open rofimoji with kaomoji",
                Self::command(&["kaomoji"], "type", None, None),
                &["kaomoji", "characters"],
            ),
            Self::item(
                "copy",
                "Emoji + Kaomoji → Clipboard",
                "Copy the selected character instead of typing it",
                Self::command(&["emojis", "kaomoji"], "copy", None, None),
                &["emoji", "kaomoji", "clipboard", "characters"],
            ),
        ];

        items.extend(Self::kaomoji_items());

        for (selector, binary) in SELECTORS {
            if Self::executable_exists(binary) {
                items.push(Self::item(
                    &format!("selector-{selector}"),
                    &format!("Emoji + Kaomoji ({selector})"),
                    &format!("Use rofimoji with the {selector} selector"),
                    Self::command(&["emojis", "kaomoji"], "type", Some(selector), None),
                    &["emoji", "kaomoji", "selector", "characters"],
                ));
            }
        }

        for (typer, binary) in TYPERS {
            if Self::executable_exists(binary) {
                items.push(Self::item(
                    &format!("typer-{}", typer.replace('-', "_")),
                    &format!("Emoji + Kaomoji (typer: {typer})"),
                    &format!("Use rofimoji with the {typer} typer"),
                    Self::command(&["emojis", "kaomoji"], "type", None, Some(typer)),
                    &["emoji", "kaomoji", "typer", "characters"],
                ));
            }
        }

        items
    }
}

impl Provider for RofimojiProvider {
    fn name(&self) -> &str {
        "Rofimoji"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        self.items = Self::build_items();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn prefix(&self) -> Option<&str> {
        Some(":rofimoji")
    }

    fn icon(&self) -> &str {
        "face-smile"
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Characters")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("emoji and kaomoji pickers")
    }
}

#[cfg(test)]
mod tests {
    use super::RofimojiProvider;

    #[test]
    fn command_contains_both_character_sets_and_action() {
        assert_eq!(
            RofimojiProvider::command(&["emojis", "kaomoji"], "type", None, None),
            "rofimoji --files emojis kaomoji --action type"
        );
    }

    #[test]
    fn command_can_pin_selector_and_typer() {
        assert_eq!(
            RofimojiProvider::command(&["emojis", "kaomoji"], "type", Some("wofi"), Some("wtype")),
            "rofimoji --files emojis kaomoji --action type --selector wofi --typer wtype"
        );
    }

    #[test]
    fn shell_quote_keeps_kaomoji_shell_safe() {
        assert_eq!(
            RofimojiProvider::shell_quote("¯\\_(ツ)_/¯"),
            "'¯\\_(ツ)_/¯'"
        );
        assert_eq!(RofimojiProvider::shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn all_requested_typers_are_integration_options() {
        let names: Vec<_> = super::TYPERS.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec!["xdotool", "wtype", "ydotool", "cliclick", "wl-ime-type"]
        );
    }

    #[test]
    fn installed_picker_exposes_the_core_modes() {
        if !RofimojiProvider::executable_exists("rofimoji") {
            return;
        }

        let items = RofimojiProvider::build_items();
        assert!(items.iter().any(|item| item.id.ends_with(":all")));
        assert!(items.iter().any(|item| item.id.ends_with(":emoji")));
        assert!(items.iter().any(|item| item.id.ends_with(":kaomoji")));
        assert!(
            items
                .iter()
                .any(|item| item.command.contains("--action copy"))
        );
        if RofimojiProvider::rofimoji_data_file().is_some() {
            assert!(items.len() > 1_000);
            assert!(items.iter().any(|item| {
                item.description
                    .as_deref()
                    .is_some_and(|description| description.contains("shrug"))
            }));
        }
    }
}
