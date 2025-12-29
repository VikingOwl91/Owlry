//! MPRIS D-Bus media player widget provider
//!
//! Shows currently playing track as a single row with play/pause action.

use super::{LaunchItem, Provider, ProviderType};
use log::debug;
use std::process::Command;

/// Media player provider using MPRIS D-Bus interface
pub struct MediaProvider {
    items: Vec<LaunchItem>,
}

#[derive(Debug, Default, Clone)]
struct MediaState {
    player_name: String,
    title: String,
    artist: String,
    is_playing: bool,
}

impl MediaProvider {
    pub fn new() -> Self {
        let mut provider = Self { items: Vec::new() };
        provider.refresh();
        provider
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
            Err(e) => {
                debug!("Failed to list D-Bus names: {}", e);
                Vec::new()
            }
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

    /// Generate single LaunchItem for media state
    fn generate_items(&mut self, state: &MediaState) {
        self.items.clear();

        let status_icon = if state.is_playing { "▶️" } else { "⏸️" };
        let action = if state.is_playing { "Pause" } else { "Play" };

        // Single row: "🎵 ▶️ Title — Artist"
        let name = format!("🎵 {} {} — {}", status_icon, state.title, state.artist);

        // Extract player display name (e.g., "firefox.instance_1_94" -> "Firefox")
        let player_display = state.player_name
            .split('.')
            .next()
            .unwrap_or(&state.player_name);
        let player_display = player_display[0..1].to_uppercase() + &player_display[1..];

        let command = format!(
            "dbus-send --session --dest=org.mpris.MediaPlayer2.{} /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player.PlayPause",
            state.player_name
        );

        self.items.push(LaunchItem {
            id: "media-now-playing".to_string(),
            name,
            description: Some(format!("{} · Press Enter to {}", player_display, action)),
            icon: Some("/org/owlry/launcher/icons/media/music-note.svg".to_string()),
            provider: ProviderType::MediaPlayer,
            command,
            terminal: false,
            tags: vec!["media".to_string(), "widget".to_string()],
        });
    }
}

impl Provider for MediaProvider {
    fn name(&self) -> &str {
        "Media Player"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::MediaPlayer
    }

    fn refresh(&mut self) {
        self.items.clear();

        let players = Self::find_players();
        if players.is_empty() {
            debug!("No MPRIS players found");
            return;
        }

        // Find first active player
        for player in &players {
            if let Some(state) = Self::get_player_state(player) {
                debug!("Found active player: {} - {}", player, state.title);
                self.generate_items(&state);
                return;
            }
        }

        debug!("No active media found");
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

impl Default for MediaProvider {
    fn default() -> Self {
        Self::new()
    }
}
