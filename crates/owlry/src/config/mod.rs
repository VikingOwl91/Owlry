use fs2::FileExt;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::paths;

/// A named profile that selects a set of provider modes.
///
/// Defined in config.toml as:
/// ```toml
/// [profiles.dev]
/// modes = ["app", "cmd", "ssh"]
///
/// [profiles.media]
/// modes = ["media", "emoji"]
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProfileConfig {
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
    /// Per-plugin configuration tables.
    /// Defined as `[plugin_config.<plugin_name>]` in config.toml.
    #[serde(default)]
    pub plugin_config: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub show_icons: bool,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Terminal command (auto-detected if not specified)
    #[serde(default)]
    pub terminal_command: Option<String>,
    /// Enable uwsm (Universal Wayland Session Manager) for launching apps.
    /// When enabled, desktop files are launched via `uwsm app -- <file>`
    /// which starts apps in a proper systemd user session.
    /// When disabled (default), apps are launched via `gio launch`.
    #[serde(default)]
    pub use_uwsm: bool,
    /// Provider tabs shown in the header bar.
    /// Valid values: app, cmd, uuctl, bookmark, calc, clip, dmenu, emoji, file, script, ssh, sys, web
    #[serde(default = "default_tabs")]
    pub tabs: Vec<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            show_icons: true,
            max_results: 100,
            terminal_command: None,
            use_uwsm: false,
            tabs: default_tabs(),
        }
    }
}

fn default_max_results() -> usize {
    100
}

fn default_tabs() -> Vec<String> {
    vec!["app".to_string(), "cmd".to_string(), "uuctl".to_string()]
}

/// User-customizable theme colors
/// All fields are optional - unset values inherit from theme or GTK defaults
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeColors {
    // Core colors
    pub background: Option<String>,
    pub background_secondary: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub text_secondary: Option<String>,
    pub accent: Option<String>,
    pub accent_bright: Option<String>,
    // Provider badge colors
    pub badge_app: Option<String>,
    pub badge_bookmark: Option<String>,
    pub badge_calc: Option<String>,
    pub badge_clip: Option<String>,
    pub badge_cmd: Option<String>,
    pub badge_dmenu: Option<String>,
    pub badge_emoji: Option<String>,
    pub badge_file: Option<String>,
    pub badge_script: Option<String>,
    pub badge_ssh: Option<String>,
    pub badge_sys: Option<String>,
    pub badge_uuctl: Option<String>,
    pub badge_web: Option<String>,
    // Widget badge colors
    pub badge_media: Option<String>,
    pub badge_weather: Option<String>,
    pub badge_pomo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_width")]
    pub width: i32,
    #[serde(default = "default_height")]
    pub height: i32,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_border_radius")]
    pub border_radius: u32,
    /// Theme name: None = GTK default, "owl" = built-in owl theme
    #[serde(default)]
    pub theme: Option<String>,
    /// Individual color overrides
    #[serde(default)]
    pub colors: ThemeColors,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            width: 850,
            height: 650,
            font_size: 14,
            border_radius: 12,
            theme: None,
            colors: ThemeColors::default(),
        }
    }
}

fn default_width() -> i32 {
    850
}
fn default_height() -> i32 {
    650
}
fn default_font_size() -> u32 {
    14
}
fn default_border_radius() -> u32 {
    12
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default = "default_true")]
    pub applications: bool,
    #[serde(default = "default_true")]
    pub commands: bool,
    /// Enable built-in calculator provider (= or calc trigger)
    #[serde(default = "default_true")]
    pub calculator: bool,
    /// Enable built-in unit/currency converter (> trigger)
    #[serde(default = "default_true")]
    pub converter: bool,
    /// Enable built-in system actions (shutdown, reboot, lock, etc.)
    #[serde(default = "default_true")]
    pub system: bool,
    /// Enable systemd user units provider (alias: uuctl)
    #[serde(default = "default_true", alias = "uuctl")]
    pub systemd: bool,
    /// Enable frecency-based result ranking
    #[serde(default = "default_true")]
    pub frecency: bool,
    /// Weight for frecency boost (0.0 = disabled, 1.0 = strong boost)
    #[serde(default = "default_frecency_weight")]
    pub frecency_weight: f64,
    /// Search engine for web search (used by owlry-plugin-websearch)
    /// Options: google, duckduckgo, bing, startpage, searxng, brave, ecosia
    /// Or a custom URL with a {query} placeholder
    #[serde(default = "default_search_engine")]
    pub search_engine: String,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            applications: true,
            commands: true,
            calculator: true,
            converter: true,
            system: true,
            systemd: true,
            frecency: true,
            frecency_weight: 0.3,
            search_engine: "duckduckgo".to_string(),
        }
    }
}

/// Configuration for plugins
///
/// Supports per-plugin configuration via `[plugins.<name>]` sections:
/// ```toml
/// [plugins]
/// enabled = true
///
/// [plugins.weather]
/// location = "Berlin"
/// units = "metric"
///
/// [plugins.pomodoro]
/// work_mins = 25
/// break_mins = 5
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsConfig {
    /// Whether plugins are enabled globally
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// List of plugin IDs to explicitly disable
    #[serde(default)]
    pub disabled_plugins: Vec<String>,

    /// Sandbox settings for plugin execution
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Plugin registry URL (for `owlry plugin search` and registry installs)
    /// Defaults to the official owlry plugin registry if not specified.
    #[serde(default)]
    pub registry_url: Option<String>,

}

/// Sandbox settings for plugin security
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Allow plugins to access the filesystem (beyond their own directory)
    #[serde(default)]
    pub allow_filesystem: bool,

    /// Allow plugins to make network requests
    #[serde(default)]
    pub allow_network: bool,

    /// Allow plugins to run shell commands
    #[serde(default)]
    pub allow_commands: bool,

    /// Memory limit for Lua runtime in bytes (0 = unlimited)
    #[serde(default = "default_memory_limit")]
    pub memory_limit: usize,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_plugins: Vec::new(),
            sandbox: SandboxConfig::default(),
            registry_url: None,
        }
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            allow_filesystem: false,
            allow_network: false,
            allow_commands: false,
            memory_limit: default_memory_limit(),
        }
    }
}

fn default_memory_limit() -> usize {
    64 * 1024 * 1024 // 64 MB
}

fn default_search_engine() -> String {
    "duckduckgo".to_string()
}

fn default_true() -> bool {
    true
}

fn default_frecency_weight() -> f64 {
    0.3
}

/// Detect the best available terminal emulator
/// Fallback chain:
/// 1. $TERMINAL env var (user's explicit preference)
/// 2. xdg-terminal-exec (freedesktop standard - if available)
/// 3. Desktop-environment native terminal (GNOME→gnome-terminal, KDE→konsole, etc.)
/// 4. Common Wayland-native terminals (kitty, alacritty, wezterm, foot)
/// 5. Common X11/legacy terminals
/// 6. x-terminal-emulator (Debian alternatives)
/// 7. xterm (ultimate fallback - the cockroach of terminals)
fn detect_terminal() -> String {
    // 1. Check $TERMINAL env var first (user's explicit preference)
    if let Ok(term) = std::env::var("TERMINAL")
        && !term.is_empty()
        && command_exists(&term)
    {
        debug!("Using $TERMINAL: {}", term);
        return term;
    }

    // 2. Try xdg-terminal-exec (freedesktop standard)
    if command_exists("xdg-terminal-exec") {
        debug!("Using xdg-terminal-exec");
        return "xdg-terminal-exec".to_string();
    }

    // 3. Desktop-environment aware detection
    if let Some(term) = detect_de_terminal() {
        debug!("Using DE-native terminal: {}", term);
        return term;
    }

    // 4. Common Wayland-native terminals (preferred for modern setups)
    let wayland_terminals = ["kitty", "alacritty", "wezterm", "foot"];
    for term in wayland_terminals {
        if command_exists(term) {
            debug!("Found Wayland terminal: {}", term);
            return term.to_string();
        }
    }

    // 5. Common X11/legacy terminals
    let legacy_terminals = [
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "mate-terminal",
        "tilix",
        "terminator",
    ];
    for term in legacy_terminals {
        if command_exists(term) {
            debug!("Found legacy terminal: {}", term);
            return term.to_string();
        }
    }

    // 6. Try x-terminal-emulator (Debian alternatives system)
    if command_exists("x-terminal-emulator") {
        debug!("Using x-terminal-emulator");
        return "x-terminal-emulator".to_string();
    }

    // 7. Ultimate fallback - xterm exists everywhere
    debug!("Falling back to xterm");
    "xterm".to_string()
}

/// Detect desktop environment and return its native terminal
fn detect_de_terminal() -> Option<String> {
    // Check XDG_CURRENT_DESKTOP first
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .map(|s| s.to_lowercase());

    // Also check for Wayland compositor-specific env vars
    let is_hyprland = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok();
    let is_sway = std::env::var("SWAYSOCK").is_ok();

    // Map desktop environments to their native/preferred terminals
    let candidates: &[&str] = if is_hyprland {
        // Hyprland: foot and kitty are most popular in the community
        &["foot", "kitty", "alacritty", "wezterm"]
    } else if is_sway {
        // Sway: foot is the recommended terminal (lightweight, Wayland-native)
        &["foot", "alacritty", "kitty", "wezterm"]
    } else if let Some(ref de) = desktop {
        match de.as_str() {
            s if s.contains("gnome") => &["gnome-terminal", "gnome-console", "kgx"],
            s if s.contains("kde") || s.contains("plasma") => &["konsole"],
            s if s.contains("xfce") => &["xfce4-terminal"],
            s if s.contains("mate") => &["mate-terminal"],
            s if s.contains("lxqt") => &["qterminal"],
            s if s.contains("lxde") => &["lxterminal"],
            s if s.contains("cinnamon") => &["gnome-terminal"],
            s if s.contains("budgie") => &["tilix", "gnome-terminal"],
            s if s.contains("pantheon") => &["io.elementary.terminal", "pantheon-terminal"],
            s if s.contains("deepin") => &["deepin-terminal"],
            s if s.contains("hyprland") => &["foot", "kitty", "alacritty", "wezterm"],
            s if s.contains("sway") => &["foot", "alacritty", "kitty", "wezterm"],
            _ => return None,
        }
    } else {
        return None;
    };

    for term in candidates {
        if command_exists(term) {
            return Some(term.to_string());
        }
    }

    None
}

/// Check if a command exists in PATH (in-process, no subprocess spawning)
fn command_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let full = dir.join(cmd);
                full.is_file()
            })
        })
        .unwrap_or(false)
}

// Note: Config derives Default via #[derive(Default)] - all sub-structs have impl Default

/// Extract leading comment lines (lines beginning with `#`) from a TOML file's content.
/// Stops at the first non-comment, non-empty line.
fn extract_header_comments(content: &str) -> String {
    let mut header = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            header.push_str(line);
            header.push('\n');
        } else {
            break;
        }
    }
    header
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        paths::config_file()
    }

    /// Get configuration table for a plugin by name.
    #[allow(dead_code)]
    pub fn get_plugin_config(&self, plugin_name: &str) -> Option<&toml::Value> {
        self.plugin_config.get(plugin_name)
    }

    /// Get a string value from a plugin's config.
    #[allow(dead_code)]
    pub fn get_plugin_string(&self, plugin_name: &str, key: &str) -> Option<&str> {
        self.plugin_config.get(plugin_name)?.get(key)?.as_str()
    }

    /// Get an integer value from a plugin's config.
    #[allow(dead_code)]
    pub fn get_plugin_int(&self, plugin_name: &str, key: &str) -> Option<i64> {
        self.plugin_config.get(plugin_name)?.get(key)?.as_integer()
    }

    /// Get a boolean value from a plugin's config.
    #[allow(dead_code)]
    pub fn get_plugin_bool(&self, plugin_name: &str, key: &str) -> Option<bool> {
        self.plugin_config.get(plugin_name)?.get(key)?.as_bool()
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_else(|e| {
            warn!("Failed to load config: {}, using defaults", e);
            Self::default()
        })
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::config_path().ok_or("Could not determine config path")?;

        let mut config = if !path.exists() {
            info!("Config file not found, using defaults");
            Self::default()
        } else {
            let content = std::fs::read_to_string(&path)?;
            let mut config: Config = toml::from_str(&content)?;
            info!("Loaded config from {:?}", path);
            // Migrate legacy [plugins.<name>] entries to [plugin_config.<name>].
            // Known PluginsConfig fields are excluded from migration.
            const KNOWN_PLUGINS_KEYS: &[&str] =
                &["enabled", "disabled_plugins", "sandbox", "registry_url"];
            if let Ok(raw) = toml::from_str::<toml::Value>(&content)
                && let Some(plugins_table) = raw.get("plugins").and_then(|v| v.as_table())
            {
                for (key, value) in plugins_table {
                    if !KNOWN_PLUGINS_KEYS.contains(&key.as_str())
                        && !config.plugin_config.contains_key(key)
                    {
                        warn!(
                            "Config: [plugins.{}] is deprecated; move to [plugin_config.{}]",
                            key, key
                        );
                        config.plugin_config.insert(key.clone(), value.clone());
                    }
                }
            }
            config
        };

        // Auto-detect terminal if not configured or configured terminal doesn't exist
        match &config.general.terminal_command {
            None => {
                let terminal = detect_terminal();
                info!("Detected terminal: {}", terminal);
                config.general.terminal_command = Some(terminal);
            }
            Some(term) if !command_exists(term) => {
                warn!("Configured terminal '{}' not found, auto-detecting", term);
                let terminal = detect_terminal();
                info!("Using detected terminal: {}", terminal);
                config.general.terminal_command = Some(terminal);
            }
            Some(term) => {
                debug!("Using configured terminal: {}", term);
            }
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path().ok_or("Could not determine config path")?;

        paths::ensure_parent_dir(&path)?;

        // Acquire an exclusive advisory lock via a sibling lock file.
        // Concurrent writers (e.g. two `owlry plugin enable` invocations) will
        // block here until the first one finishes, preventing interleaved writes.
        let lock_path = path.with_extension("toml.lock");
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false) // lock files are never written to; don't clobber existing
            .open(&lock_path)?;
        lock_file.lock_exclusive()?;

        // Preserve any leading comment block (e.g. user docs / generated header).
        let header = if path.exists() {
            let existing = std::fs::read_to_string(&path)?;
            extract_header_comments(&existing)
        } else {
            String::new()
        };

        let body = toml::to_string_pretty(self)?;
        let content = if header.is_empty() {
            body
        } else {
            format!("{}\n{}", header.trim_end(), body)
        };

        // Atomic write: write to a sibling temp file, then rename over the target.
        // rename(2) is atomic on POSIX — readers always see either the old or new file.
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &content)?;
        std::fs::rename(&tmp_path, &path)?;

        // Lock is released when lock_file is dropped here.
        drop(lock_file);

        info!("Saved config to {:?}", path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_exists_finds_sh() {
        // /bin/sh exists on every Unix system
        assert!(super::command_exists("sh"));
    }

    #[test]
    fn command_exists_rejects_nonexistent() {
        assert!(!super::command_exists("owlry_nonexistent_binary_abc123"));
    }
}
