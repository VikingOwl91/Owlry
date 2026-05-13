//! Clipboard history provider.
//!
//! Static provider that surfaces `cliphist` entries as launch items. Selecting
//! an item runs `cliphist decode | wl-copy` to put the entry back on the
//! clipboard. Requires `cliphist` and `wl-clipboard` to be installed.

use super::{ItemSource, LaunchItem, Provider, ProviderType};
use std::process::Command;

const TYPE_ID: &str = "clipboard";
const PROVIDER_ICON: &str = "edit-paste";
const DEFAULT_MAX_ENTRIES: usize = 50;

pub struct ClipboardProvider {
    items: Vec<LaunchItem>,
    max_entries: usize,
}

impl Default for ClipboardProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardProvider {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    fn has_cliphist() -> bool {
        Command::new("which")
            .arg("cliphist")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_cliphist_output(output: &str, max_entries: usize) -> Vec<LaunchItem> {
        let mut items = Vec::new();

        for (idx, line) in output.lines().take(max_entries).enumerate() {
            // cliphist format: "id\tpreview"
            let parts: Vec<&str> = line.splitn(2, '\t').collect();

            if parts.is_empty() {
                continue;
            }

            let clip_id = parts[0];
            let preview = if parts.len() > 1 {
                // Truncate long previews (char-safe for UTF-8)
                let p = parts[1];
                if p.chars().count() > 80 {
                    let truncated: String = p.chars().take(77).collect();
                    format!("{}...", truncated)
                } else {
                    p.to_string()
                }
            } else {
                "[binary data]".to_string()
            };

            // Clean up preview - replace newlines/tabs with spaces, strip CR.
            let preview_clean = preview
                .replace('\n', " ")
                .replace('\r', "")
                .replace('\t', " ");

            // Command to paste this entry: echo "id" | cliphist decode | wl-copy
            let command = format!(
                "echo '{}' | cliphist decode | wl-copy",
                clip_id.replace('\'', "'\\''")
            );

            items.push(LaunchItem {
                id: format!("clipboard:{}", idx),
                name: preview_clean,
                description: Some("Copy to clipboard".to_string()),
                icon: Some(PROVIDER_ICON.to_string()),
                provider: ProviderType::Plugin(TYPE_ID.into()),
                command,
                terminal: false,
                tags: vec!["clipboard".to_string()],
                source: ItemSource::Core,
            });
        }

        items
    }
}

impl Provider for ClipboardProvider {
    fn name(&self) -> &str {
        "Clipboard"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        self.items.clear();

        if !Self::has_cliphist() {
            return;
        }

        let output = match Command::new("cliphist").arg("list").output() {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.items = Self::parse_cliphist_output(&stdout, self.max_entries);
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn prefix(&self) -> Option<&str> {
        Some(":clip")
    }

    fn icon(&self) -> &str {
        PROVIDER_ICON
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Clipboard")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("clipboard entries")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_provider_new() {
        let p = ClipboardProvider::new();
        assert!(p.items.is_empty());
        assert_eq!(p.max_entries, DEFAULT_MAX_ENTRIES);
    }

    #[test]
    fn test_preview_truncation() {
        // Long ASCII strings get truncated char-safely to 80 chars with "..." suffix.
        let long_text = "a".repeat(100);
        let truncated = if long_text.chars().count() > 80 {
            let t: String = long_text.chars().take(77).collect();
            format!("{}...", t)
        } else {
            long_text.clone()
        };
        assert_eq!(truncated.chars().count(), 80);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_preview_truncation_utf8() {
        // Multi-byte UTF-8 chars must not split mid-codepoint.
        let utf8_text = "├── ".repeat(30);
        let truncated = if utf8_text.chars().count() > 80 {
            let t: String = utf8_text.chars().take(77).collect();
            format!("{}...", t)
        } else {
            utf8_text.clone()
        };
        assert_eq!(truncated.chars().count(), 80);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_preview_cleaning() {
        let dirty = "line1\nline2\tcolumn\rend";
        let clean = dirty
            .replace('\n', " ")
            .replace('\r', "")
            .replace('\t', " ");
        assert_eq!(clean, "line1 line2 columnend");
    }

    #[test]
    fn test_command_escaping() {
        let clip_id = "test'id";
        let command = format!(
            "echo '{}' | cliphist decode | wl-copy",
            clip_id.replace('\'', "'\\''")
        );
        assert!(command.contains("test'\\''id"));
    }

    #[test]
    fn test_has_cliphist_runs() {
        // Just ensure it doesn't panic — cliphist may or may not be installed.
        let _ = ClipboardProvider::has_cliphist();
    }

    #[test]
    fn parse_cliphist_output_builds_items_with_plugin_provider_type() {
        let output = "1\thello world\n2\tanother entry\n";
        let items = ClipboardProvider::parse_cliphist_output(output, DEFAULT_MAX_ENTRIES);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "hello world");
        assert_eq!(items[0].provider, ProviderType::Plugin("clipboard".into()));
        assert_eq!(items[0].source, ItemSource::Core);
        assert!(items[0].command.contains("cliphist decode"));
        assert!(items[0].command.contains("wl-copy"));
        assert!(!items[0].terminal);
    }

    #[test]
    fn parse_cliphist_output_respects_max_entries() {
        let output = "1\ta\n2\tb\n3\tc\n4\td\n";
        let items = ClipboardProvider::parse_cliphist_output(output, 2);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn parse_cliphist_output_handles_missing_preview_as_binary() {
        let output = "42\n";
        let items = ClipboardProvider::parse_cliphist_output(output, DEFAULT_MAX_ENTRIES);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "[binary data]");
    }

    #[test]
    fn provider_type_is_clipboard_plugin() {
        // CLI back-compat: `-m clip` / `:clip` resolve to this type_id.
        let p = ClipboardProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("clipboard".into()));
    }

    #[test]
    fn provider_metadata_exposes_clip_prefix_and_labels() {
        let p = ClipboardProvider::new();
        assert_eq!(p.prefix(), Some(":clip"));
        assert_eq!(p.icon(), "edit-paste");
        assert_eq!(p.tab_label(), Some("Clipboard"));
        assert_eq!(p.search_noun(), Some("clipboard entries"));
    }
}
