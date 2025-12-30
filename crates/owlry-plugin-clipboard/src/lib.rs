//! Clipboard Plugin for Owlry
//!
//! A static provider that integrates with cliphist to show clipboard history.
//! Requires cliphist and wl-clipboard to be installed.
//!
//! Dependencies:
//! - cliphist: clipboard history manager
//! - wl-clipboard: Wayland clipboard utilities (wl-copy)

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind,
    ProviderPosition, API_VERSION,
};
use std::process::Command;

// Plugin metadata
const PLUGIN_ID: &str = "clipboard";
const PLUGIN_NAME: &str = "Clipboard";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Clipboard history via cliphist";

// Provider metadata
const PROVIDER_ID: &str = "clipboard";
const PROVIDER_NAME: &str = "Clipboard";
const PROVIDER_PREFIX: &str = ":clip";
const PROVIDER_ICON: &str = "edit-paste";
const PROVIDER_TYPE_ID: &str = "clipboard";

// Default max entries to show
const DEFAULT_MAX_ENTRIES: usize = 50;

/// Clipboard provider state - holds cached items
struct ClipboardState {
    items: Vec<PluginItem>,
    max_entries: usize,
}

impl ClipboardState {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
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
            return;
        }

        // Get clipboard history from cliphist
        let output = match Command::new("cliphist").arg("list").output() {
            Ok(o) => o,
            Err(_) => return,
        };

        if !output.status.success() {
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

            self.items.push(
                PluginItem::new(format!("clipboard:{}", idx), preview_clean, command)
                    .with_description("Copy to clipboard")
                    .with_icon(PROVIDER_ICON),
            );
        }
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
    let state = Box::new(ClipboardState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<ClipboardState>
    let state = unsafe { &mut *(handle.ptr as *mut ClipboardState) };

    // Load clipboard history
    state.load_clipboard_history();

    // Return items
    state.items.to_vec().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query is handled by the core using cached items
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<ClipboardState>
        unsafe {
            handle.drop_as::<ClipboardState>();
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
    fn test_clipboard_state_new() {
        let state = ClipboardState::new();
        assert!(state.items.is_empty());
        assert_eq!(state.max_entries, DEFAULT_MAX_ENTRIES);
    }

    #[test]
    fn test_preview_truncation() {
        // Test that long strings would be truncated (char-safe)
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
        // Test with multi-byte UTF-8 characters (box-drawing chars are 3 bytes each)
        let utf8_text = "├── ".repeat(30); // Each "├── " is 7 bytes but 4 chars
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
        // Just ensure it doesn't panic - cliphist may or may not be installed
        let _ = ClipboardState::has_cliphist();
    }
}
