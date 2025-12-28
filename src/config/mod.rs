use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use log::{info, warn, debug};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    pub width: i32,
    pub height: i32,
    pub font_size: u32,
    pub border_radius: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub applications: bool,
    pub commands: bool,
    pub uuctl: bool,
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
            },
            appearance: AppearanceConfig {
                width: 600,
                height: 400,
                font_size: 14,
                border_radius: 12,
            },
            providers: ProvidersConfig {
                applications: true,
                commands: true,
                uuctl: true,
            },
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("owlry").join("config.toml"))
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

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path().ok_or("Could not determine config path")?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        info!("Saved config to {:?}", path);
        Ok(())
    }
}
