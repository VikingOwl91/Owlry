use crate::providers::{LaunchItem, Provider, ProviderType};
use log::{debug, warn};
use std::process::Command;

/// Clipboard history provider - integrates with cliphist
pub struct ClipboardProvider {
    items: Vec<LaunchItem>,
    max_entries: usize,
}

impl ClipboardProvider {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max_entries: 50,
        }
    }

    #[allow(dead_code)]
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            items: Vec::new(),
            max_entries,
        }
    }

    /// Check if cliphist is available
    fn has_cliphist() -> bool {
        Command::new("which")
            .arg("cliphist")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn load_clipboard_history(&mut self) {
        self.items.clear();

        if !Self::has_cliphist() {
            debug!("cliphist not found, clipboard provider disabled");
            return;
        }

        // Get clipboard history from cliphist
        let output = match Command::new("cliphist").arg("list").output() {
            Ok(o) => o,
            Err(e) => {
                warn!("Failed to run cliphist: {}", e);
                return;
            }
        };

        if !output.status.success() {
            debug!("cliphist list returned non-zero status");
            return;
        }

        let content = String::from_utf8_lossy(&output.stdout);

        for (idx, line) in content.lines().take(self.max_entries).enumerate() {
            // cliphist format: "id\tpreview"
            let parts: Vec<&str> = line.splitn(2, '\t').collect();

            if parts.is_empty() {
                continue;
            }

            let clip_id = parts[0];
            let preview = if parts.len() > 1 {
                // Truncate long previews
                let p = parts[1];
                if p.len() > 80 {
                    format!("{}...", &p[..77])
                } else {
                    p.to_string()
                }
            } else {
                "[binary data]".to_string()
            };

            // Clean up preview - replace newlines with spaces
            let preview_clean = preview
                .replace('\n', " ")
                .replace('\r', "")
                .replace('\t', " ");

            // Command to paste this entry
            // echo "id" | cliphist decode | wl-copy
            let command = format!(
                "echo '{}' | cliphist decode | wl-copy",
                clip_id.replace('\'', "'\\''")
            );

            self.items.push(LaunchItem {
                id: format!("clipboard:{}", idx),
                name: preview_clean,
                description: Some("Copy to clipboard".to_string()),
                icon: Some("edit-paste".to_string()),
                provider: ProviderType::Clipboard,
                command,
                terminal: false,
                tags: Vec::new(),
            });
        }

        debug!("Loaded {} clipboard entries", self.items.len());
    }
}

impl Provider for ClipboardProvider {
    fn name(&self) -> &str {
        "Clipboard"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Clipboard
    }

    fn refresh(&mut self) {
        self.load_clipboard_history();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_provider() {
        let mut provider = ClipboardProvider::new();
        provider.refresh();
        // Just ensure it doesn't panic - cliphist may not be installed
    }
}
