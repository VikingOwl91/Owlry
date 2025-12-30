//! MPRIS Media Player Widget Plugin for Owlry
//!
//! Shows currently playing track as a single row with play/pause action.
//! Uses D-Bus via dbus-send to communicate with MPRIS-compatible players.

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};
use std::process::Command;

// Plugin metadata
const PLUGIN_ID: &str = "media";
const PLUGIN_NAME: &str = "Media Player";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "MPRIS media player widget - shows and controls currently playing media";

// Provider metadata
const PROVIDER_ID: &str = "media";
const PROVIDER_NAME: &str = "Media";
const PROVIDER_ICON: &str = "applications-multimedia";
const PROVIDER_TYPE_ID: &str = "media";

#[derive(Debug, Default, Clone)]
struct MediaState {
    player_name: String,
    title: String,
    artist: String,
    is_playing: bool,
}

/// Media provider state
struct MediaProviderState {
    items: Vec<PluginItem>,
    /// Current player name for submenu actions
    current_player: Option<String>,
    /// Current playback state
    is_playing: bool,
}

impl MediaProviderState {
    fn new() -> Self {
        // Don't query D-Bus during init - defer to first refresh() call
        // This prevents blocking the main thread during startup
        Self {
            items: Vec::new(),
            current_player: None,
            is_playing: false,
        }
    }

    fn refresh(&mut self) {
        self.items.clear();

        let players = Self::find_players();
        if players.is_empty() {
            return;
        }

        // Find first active player
        for player in &players {
            if let Some(state) = Self::get_player_state(player) {
                self.generate_items(&state);
                return;
            }
        }
    }

    /// Find active MPRIS players via dbus-send
    fn find_players() -> Vec<String> {
        let output = Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.DBus",
                "--type=method_call",
                "--print-reply",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.ListNames",
            ])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout
                    .lines()
                    .filter_map(|line| {
                        let trimmed = line.trim();
                        if trimmed.starts_with("string \"org.mpris.MediaPlayer2.") {
                            let start = "string \"org.mpris.MediaPlayer2.".len();
                            let end = trimmed.len() - 1;
                            Some(trimmed[start..end].to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Get metadata from an MPRIS player
    fn get_player_state(player: &str) -> Option<MediaState> {
        let dest = format!("org.mpris.MediaPlayer2.{}", player);

        // Get playback status
        let status_output = Command::new("dbus-send")
            .args([
                "--session",
                &format!("--dest={}", dest),
                "--type=method_call",
                "--print-reply",
                "/org/mpris/MediaPlayer2",
                "org.freedesktop.DBus.Properties.Get",
                "string:org.mpris.MediaPlayer2.Player",
                "string:PlaybackStatus",
            ])
            .output()
            .ok()?;

        let status_str = String::from_utf8_lossy(&status_output.stdout);
        let is_playing = status_str.contains("\"Playing\"");
        let is_paused = status_str.contains("\"Paused\"");

        // Only show if playing or paused (not stopped)
        if !is_playing && !is_paused {
            return None;
        }

        // Get metadata
        let metadata_output = Command::new("dbus-send")
            .args([
                "--session",
                &format!("--dest={}", dest),
                "--type=method_call",
                "--print-reply",
                "/org/mpris/MediaPlayer2",
                "org.freedesktop.DBus.Properties.Get",
                "string:org.mpris.MediaPlayer2.Player",
                "string:Metadata",
            ])
            .output()
            .ok()?;

        let metadata_str = String::from_utf8_lossy(&metadata_output.stdout);

        let title = Self::extract_string(&metadata_str, "xesam:title")
            .unwrap_or_else(|| "Unknown".to_string());
        let artist = Self::extract_array(&metadata_str, "xesam:artist")
            .unwrap_or_else(|| "Unknown".to_string());

        Some(MediaState {
            player_name: player.to_string(),
            title,
            artist,
            is_playing,
        })
    }

    /// Extract string value from D-Bus output
    fn extract_string(output: &str, key: &str) -> Option<String> {
        let key_pattern = format!("\"{}\"", key);
        let mut found = false;

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.contains(&key_pattern) {
                found = true;
                continue;
            }
            if found {
                if let Some(pos) = trimmed.find("string \"") {
                    let start = pos + "string \"".len();
                    if let Some(end) = trimmed[start..].find('"') {
                        let value = &trimmed[start..start + end];
                        if !value.is_empty() {
                            return Some(value.to_string());
                        }
                    }
                }
                if !trimmed.starts_with("variant") {
                    found = false;
                }
            }
        }
        None
    }

    /// Extract array value from D-Bus output
    fn extract_array(output: &str, key: &str) -> Option<String> {
        let key_pattern = format!("\"{}\"", key);
        let mut found = false;
        let mut in_array = false;
        let mut values = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.contains(&key_pattern) {
                found = true;
                continue;
            }
            if found && trimmed.contains("array [") {
                in_array = true;
                continue;
            }
            if in_array {
                if let Some(pos) = trimmed.find("string \"") {
                    let start = pos + "string \"".len();
                    if let Some(end) = trimmed[start..].find('"') {
                        values.push(trimmed[start..start + end].to_string());
                    }
                }
                if trimmed.contains(']') {
                    break;
                }
            }
        }

        if values.is_empty() {
            None
        } else {
            Some(values.join(", "))
        }
    }

    /// Generate single LaunchItem for media state (opens submenu)
    fn generate_items(&mut self, state: &MediaState) {
        self.items.clear();

        // Store state for submenu
        self.current_player = Some(state.player_name.clone());
        self.is_playing = state.is_playing;

        // Single row: "Title — Artist"
        let name = format!("{} — {}", state.title, state.artist);

        // Extract player display name (e.g., "firefox.instance_1_94" -> "Firefox")
        let player_display = Self::format_player_name(&state.player_name);

        // Opens submenu with media controls
        self.items.push(
            PluginItem::new("media-now-playing", name, "SUBMENU:media:controls")
                .with_description(format!("{} · Select for controls", player_display))
                .with_icon("/org/owlry/launcher/icons/media/music-note.svg")
                .with_keywords(vec!["media".to_string(), "widget".to_string()]),
        );
    }

    /// Format player name for display
    fn format_player_name(player_name: &str) -> String {
        let player_display = player_name.split('.').next().unwrap_or(player_name);
        if player_display.is_empty() {
            "Player".to_string()
        } else {
            let mut chars = player_display.chars();
            match chars.next() {
                None => "Player".to_string(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        }
    }

    /// Generate submenu items for media controls
    fn generate_submenu_items(&self) -> Vec<PluginItem> {
        let player = match &self.current_player {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut items = Vec::new();

        // Use playerctl for simpler, more reliable media control
        // playerctl -p <player> <command>

        // Play/Pause
        if self.is_playing {
            items.push(
                PluginItem::new(
                    "media-pause",
                    "Pause",
                    format!("playerctl -p {} pause", player),
                )
                .with_description("Pause playback")
                .with_icon("media-playback-pause"),
            );
        } else {
            items.push(
                PluginItem::new(
                    "media-play",
                    "Play",
                    format!("playerctl -p {} play", player),
                )
                .with_description("Resume playback")
                .with_icon("media-playback-start"),
            );
        }

        // Next track
        items.push(
            PluginItem::new(
                "media-next",
                "Next",
                format!("playerctl -p {} next", player),
            )
            .with_description("Skip to next track")
            .with_icon("media-skip-forward"),
        );

        // Previous track
        items.push(
            PluginItem::new(
                "media-previous",
                "Previous",
                format!("playerctl -p {} previous", player),
            )
            .with_description("Go to previous track")
            .with_icon("media-skip-backward"),
        );

        // Stop
        items.push(
            PluginItem::new(
                "media-stop",
                "Stop",
                format!("playerctl -p {} stop", player),
            )
            .with_description("Stop playback")
            .with_icon("media-playback-stop"),
        );

        items
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
        prefix: ROption::RNone,
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Static,
        type_id: RString::from(PROVIDER_TYPE_ID),
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(MediaProviderState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<MediaProviderState>
    let state = unsafe { &mut *(handle.ptr as *mut MediaProviderState) };

    state.refresh();
    state.items.clone().into()
}

extern "C" fn provider_query(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    let query_str = query.as_str();
    let state = unsafe { &*(handle.ptr as *const MediaProviderState) };

    // Handle submenu request
    if query_str == "?SUBMENU:controls" {
        return state.generate_submenu_items().into();
    }

    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<MediaProviderState>
        unsafe {
            handle.drop_as::<MediaProviderState>();
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
    fn test_extract_string() {
        let output = r#"
            string "xesam:title"
            variant       string "My Song Title"
        "#;
        assert_eq!(
            MediaProviderState::extract_string(output, "xesam:title"),
            Some("My Song Title".to_string())
        );
    }

    #[test]
    fn test_extract_array() {
        let output = r#"
            string "xesam:artist"
            variant       array [
                string "Artist One"
                string "Artist Two"
            ]
        "#;
        assert_eq!(
            MediaProviderState::extract_array(output, "xesam:artist"),
            Some("Artist One, Artist Two".to_string())
        );
    }

    #[test]
    fn test_extract_string_not_found() {
        let output = "some other output";
        assert_eq!(
            MediaProviderState::extract_string(output, "xesam:title"),
            None
        );
    }

    #[test]
    fn test_find_players_empty() {
        // This will return empty on systems without D-Bus
        let players = MediaProviderState::find_players();
        // Just verify it doesn't panic
        let _ = players;
    }
}
