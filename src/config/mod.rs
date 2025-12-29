use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub appearance: AppearanceConfig,
    pub providers: ProvidersConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub show_icons: bool,
    pub max_results: usize,
    pub terminal_command: String,
    /// Launch wrapper command for app execution.
    /// Examples: "uwsm app --", "hyprctl dispatch exec --", "systemd-run --user --"
    /// If None or empty, launches directly via sh -c
    #[serde(default)]
    pub launch_wrapper: Option<String>,
    /// Provider tabs shown in the header bar.
    /// Valid values: app, cmd, uuctl, bookmark, calc, clip, dmenu, emoji, file, script, ssh, sys, web
    #[serde(default = "default_tabs")]
    pub tabs: Vec<String>,
}

fn default_tabs() -> Vec<String> {
    vec![
        "app".to_string(),
        "cmd".to_string(),
        "uuctl".to_string(),
    ]
}

/// User-customizable theme colors
/// All fields are optional - unset values inherit from GTK theme
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeColors {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    pub width: i32,
    pub height: i32,
    pub font_size: u32,
    pub border_radius: u32,
    /// Theme name: None = GTK default, "owl" = built-in owl theme
    #[serde(default)]
    pub theme: Option<String>,
    /// Individual color overrides
    #[serde(default)]
    pub colors: ThemeColors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub applications: bool,
    pub commands: bool,
    pub uuctl: bool,
    /// Enable calculator provider (= expression or calc expression)
    #[serde(default = "default_true")]
    pub calculator: bool,
    /// Enable frecency-based result ranking
    #[serde(default = "default_true")]
    pub frecency: bool,
    /// Weight for frecency boost (0.0 = disabled, 1.0 = strong boost)
    #[serde(default = "default_frecency_weight")]
    pub frecency_weight: f64,
    /// Enable web search provider (? query or web query)
    #[serde(default = "default_true")]
    pub websearch: bool,
    /// Search engine for web search
    /// Options: google, duckduckgo, bing, startpage, searxng, brave, ecosia
    /// Or custom URL with {query} placeholder
    #[serde(default = "default_search_engine")]
    pub search_engine: String,
    /// Enable system commands (shutdown, reboot, etc.)
    #[serde(default = "default_true")]
    pub system: bool,
    /// Enable SSH connections from ~/.ssh/config
    #[serde(default = "default_true")]
    pub ssh: bool,
    /// Enable clipboard history (requires cliphist)
    #[serde(default = "default_true")]
    pub clipboard: bool,
    /// Enable browser bookmarks
    #[serde(default = "default_true")]
    pub bookmarks: bool,
    /// Enable emoji picker
    #[serde(default = "default_true")]
    pub emoji: bool,
    /// Enable custom scripts from ~/.config/owlry/scripts/
    #[serde(default = "default_true")]
    pub scripts: bool,
    /// Enable file search (requires fd or locate)
    #[serde(default = "default_true")]
    pub files: bool,
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

/// Detect the best launch wrapper for the current session
/// Checks for uwsm (Universal Wayland Session Manager) and hyprland
fn detect_launch_wrapper() -> Option<String> {
    // Check if running under uwsm (has UWSM_FINALIZE_VARNAMES or similar uwsm env vars)
    if std::env::var("UWSM_FINALIZE_VARNAMES").is_ok()
        || std::env::var("__UWSM_SELECT_TAG").is_ok()
    {
        if command_exists("uwsm") {
            debug!("Detected uwsm session, using 'uwsm app --' wrapper");
            return Some("uwsm app --".to_string());
        }
    }

    // Check if running under Hyprland
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        if command_exists("hyprctl") {
            debug!("Detected Hyprland session, using 'hyprctl dispatch exec --' wrapper");
            return Some("hyprctl dispatch exec --".to_string());
        }
    }

    // No wrapper needed for other environments
    debug!("No launch wrapper detected, using direct execution");
    None
}

/// Detect the best available terminal emulator
/// Fallback chain:
/// 1. $TERMINAL env var (user's explicit preference)
/// 2. xdg-terminal-exec (freedesktop standard)
/// 3. Common Wayland-native terminals (kitty, alacritty, wezterm, foot)
/// 4. Common X11/legacy terminals (gnome-terminal, konsole, xfce4-terminal)
/// 5. x-terminal-emulator (Debian alternatives)
/// 6. xterm (ultimate fallback)
fn detect_terminal() -> String {
    // 1. Check $TERMINAL env var first
    if let Ok(term) = std::env::var("TERMINAL") {
        if !term.is_empty() && command_exists(&term) {
            debug!("Using $TERMINAL: {}", term);
            return term;
        }
    }

    // 2. Try xdg-terminal-exec (freedesktop standard)
    if command_exists("xdg-terminal-exec") {
        debug!("Using xdg-terminal-exec");
        return "xdg-terminal-exec".to_string();
    }

    // 3. Common Wayland-native terminals (preferred)
    let wayland_terminals = ["kitty", "alacritty", "wezterm", "foot"];
    for term in wayland_terminals {
        if command_exists(term) {
            debug!("Found Wayland terminal: {}", term);
            return term.to_string();
        }
    }

    // 4. Common X11/legacy terminals
    let legacy_terminals = ["gnome-terminal", "konsole", "xfce4-terminal", "tilix", "terminator"];
    for term in legacy_terminals {
        if command_exists(term) {
            debug!("Found legacy terminal: {}", term);
            return term.to_string();
        }
    }

    // 5. Try x-terminal-emulator (Debian alternatives system)
    if command_exists("x-terminal-emulator") {
        debug!("Using x-terminal-emulator");
        return "x-terminal-emulator".to_string();
    }

    // 6. Ultimate fallback
    debug!("Falling back to xterm");
    "xterm".to_string()
}

/// Check if a command exists in PATH
fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

impl Default for Config {
    fn default() -> Self {
        let terminal = detect_terminal();
        info!("Detected terminal: {}", terminal);

        Self {
            general: GeneralConfig {
                show_icons: true,
                max_results: 10,
                terminal_command: terminal,
                launch_wrapper: detect_launch_wrapper(),
                tabs: default_tabs(),
            },
            appearance: AppearanceConfig {
                width: 700,
                height: 500,
                font_size: 14,
                border_radius: 12,
                theme: None,
                colors: ThemeColors::default(),
            },
            providers: ProvidersConfig {
                applications: true,
                commands: true,
                uuctl: true,
                calculator: true,
                frecency: true,
                frecency_weight: 0.3,
                websearch: true,
                search_engine: "duckduckgo".to_string(),
                system: true,
                ssh: true,
                clipboard: true,
                bookmarks: true,
                emoji: true,
                scripts: true,
                files: true,
            },
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        paths::config_file()
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_else(|e| {
            warn!("Failed to load config: {}, using defaults", e);
            Self::default()
        })
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::config_path().ok_or("Could not determine config path")?;

        if !path.exists() {
            info!("Config file not found, using defaults");
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let mut config: Config = toml::from_str(&content)?;
        info!("Loaded config from {:?}", path);

        // Validate terminal - if configured terminal doesn't exist, auto-detect
        if !command_exists(&config.general.terminal_command) {
            warn!(
                "Configured terminal '{}' not found, auto-detecting",
                config.general.terminal_command
            );
            config.general.terminal_command = detect_terminal();
            info!("Using detected terminal: {}", config.general.terminal_command);
        }

        Ok(config)
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path().ok_or("Could not determine config path")?;

        paths::ensure_parent_dir(&path)?;

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        info!("Saved config to {:?}", path);
        Ok(())
    }
}
