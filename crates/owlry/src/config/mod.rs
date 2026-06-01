use fs2::FileExt;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    #[serde(alias = "badge_sys")]
    pub badge_power: Option<String>,
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
    /// Enable built-in power actions (shutdown, reboot, lock, etc.)
    /// Pre-v2 config key `system` is still accepted as an alias.
    #[serde(default = "default_true", alias = "system", alias = "sys")]
    pub power: bool,
    /// Enable systemd user units provider (alias: uuctl)
    #[serde(default = "default_true", alias = "uuctl")]
    pub systemd: bool,
    /// Enable clipboard history provider (requires cliphist)
    #[serde(default = "default_true")]
    pub clipboard: bool,
    /// Enable emoji picker provider
    #[serde(default = "default_true")]
    pub emoji: bool,
    /// Enable filesystem search provider (uses fd or mlocate)
    #[serde(default = "default_true")]
    pub filesearch: bool,
    /// Enable SSH host provider (parses ~/.ssh/config)
    #[serde(default = "default_true")]
    pub ssh: bool,
    /// Enable web search provider (DuckDuckGo / configurable)
    #[serde(default = "default_true")]
    pub websearch: bool,
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
    /// Root directories for file search (`:file` / `/`). Each entry may use a
    /// leading `~/` for the home directory. Empty (the default) searches
    /// `$HOME`.
    #[serde(default)]
    pub filesearch_roots: Vec<String>,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            applications: true,
            commands: true,
            calculator: true,
            converter: true,
            power: true,
            systemd: true,
            clipboard: true,
            emoji: true,
            filesearch: true,
            ssh: true,
            websearch: true,
            frecency: true,
            frecency_weight: 0.3,
            search_engine: "duckduckgo".to_string(),
            filesearch_roots: Vec::new(),
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

    /// Resolve the user's effective config. Order per `docs/lua-api.md` §2:
    ///
    /// 1. `$XDG_CONFIG_HOME/owlry/owlry.lua` — when present, defaults are the
    ///    base and Lua overlays. TOML is ignored entirely with an info log.
    /// 2. `$XDG_CONFIG_HOME/owlry/config.toml` — back-compat path.
    /// 3. Built-in defaults.
    ///
    /// The terminal command is auto-detected at the end regardless of source.
    ///
    /// Lua-defined user providers from `owlry.provider {}` are dropped here
    /// (the [`crate::lua::LuaContext`] is released after merge). Callers that
    /// need them — the daemon, hot-reload — must use [`LoadedConfig::load`]
    /// instead, which keeps the context alive.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "lua")]
        {
            if let Some(lua_path) = paths::lua_config_file()
                && lua_path.exists()
            {
                let mut cfg = Self::load_via_lua(&lua_path)?;
                detect_and_apply_terminal(&mut cfg);
                return Ok(cfg);
            }
        }

        let mut cfg = Self::load_from_toml(Self::config_path().as_deref())?;
        detect_and_apply_terminal(&mut cfg);
        Ok(cfg)
    }

    /// TOML-only load path. Honours the legacy `[plugins.<name>]` →
    /// `[plugin_config.<name>]` migration. Returns defaults if the file is
    /// missing or the path is `None`.
    pub(crate) fn load_from_toml(
        path: Option<&Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let Some(path) = path else {
            info!("No TOML config path available; using defaults");
            return Ok(Self::default());
        };

        if !path.exists() {
            info!("Config file not found, using defaults");
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
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
        Ok(config)
    }

    /// Evaluate `owlry.lua` and overlay it onto defaults. TOML is ignored
    /// per spec §2. Drops the LuaContext immediately — user providers won't
    /// survive past this call (use [`LoadedConfig::load`] for those).
    #[cfg(feature = "lua")]
    fn load_via_lua(lua_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let loaded = LoadedConfig::load_lua_path(lua_path)?;
        Ok(loaded.config)
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

/// Auto-detect and apply the terminal command on a [`Config`]. Runs after
/// every load path (Lua, TOML, defaults) so the effective config always has
/// a working terminal — even when neither file specified one.
fn detect_and_apply_terminal(config: &mut Config) {
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
}

// =============================================================================
// LoadedConfig — full resolution result with the Lua runtime kept alive
// =============================================================================
//
// Read this before touching the type: the daemon needs the LuaContext to
// outlive the user-defined providers it captured (their `items` closures
// reference Lua state). [`Config::load`] discards the context after merging
// scalars; daemon and hot-reload paths must use [`LoadedConfig::load`] to
// hold it open.

/// Full result of config resolution: the effective [`Config`] plus the
/// optional Lua runtime + captured user-provider specs.
///
/// Behaviour rules:
/// - `lua` and `lua_path` are `Some` only when `owlry.lua` was found and
///   evaluated successfully. Otherwise both are `None` and `user_providers`
///   is empty.
/// - When `lua` is present, the base [`Config`] is built from defaults and
///   overlaid with Lua state — TOML is intentionally ignored (spec §2).
///   An info log is emitted if a `config.toml` is sitting next to it.
#[cfg(feature = "lua")]
pub struct LoadedConfig {
    pub config: Config,
    pub lua: Option<crate::lua::LuaContext>,
    pub lua_path: Option<PathBuf>,
    pub user_providers: Vec<crate::lua::LuaProviderSpec>,
}

#[cfg(feature = "lua")]
impl std::fmt::Debug for LoadedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedConfig")
            .field("config", &"<Config>")
            .field("lua", &self.lua.as_ref().map(|_| "<LuaContext>"))
            .field("lua_path", &self.lua_path)
            .field("user_providers", &self.user_providers)
            .finish()
    }
}

#[cfg(feature = "lua")]
impl LoadedConfig {
    /// Build a [`LoadedConfig`] containing only defaults + an auto-detected
    /// terminal. Useful for tests and the `load_or_default` fallback.
    pub fn defaults() -> Self {
        let mut config = Config::default();
        detect_and_apply_terminal(&mut config);
        Self {
            config,
            lua: None,
            lua_path: None,
            user_providers: Vec::new(),
        }
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_else(|e| {
            warn!("Failed to load config: {}; using defaults", e);
            Self::defaults()
        })
    }

    /// Resolve via the full Lua → TOML → defaults order. See
    /// [`Config::load`] for the same precedence semantics; the difference
    /// is that this variant preserves the Lua runtime and user providers.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(lua_path) = paths::lua_config_file()
            && lua_path.exists()
        {
            return Self::load_lua_path(&lua_path);
        }

        // No Lua config — fall back to TOML or defaults.
        let mut config = Config::load_from_toml(Config::config_path().as_deref())?;
        detect_and_apply_terminal(&mut config);
        Ok(Self {
            config,
            lua: None,
            lua_path: None,
            user_providers: Vec::new(),
        })
    }

    /// Internal: evaluate a specific Lua file and produce a LoadedConfig.
    /// Used by both the public load path and [`Config::load_via_lua`].
    pub(crate) fn load_lua_path(lua_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        // If a TOML config sits alongside the Lua file, surface that we're
        // ignoring it. Per spec §2: "TOML is ignored entirely when Lua is
        // present." A 2.2 release will upgrade this to a warning; 3.0
        // removes TOML support entirely.
        if let Some(toml_path) = paths::config_file()
            && toml_path.exists()
        {
            info!(
                "owlry.lua present at {:?} — ignoring {:?} (Lua takes precedence)",
                lua_path, toml_path
            );
        }

        let ctx = crate::lua::LuaContext::new()?;
        ctx.eval_file(lua_path)?;
        let lua_cfg = ctx.snapshot();

        let mut config = Config::default();
        lua_cfg.merge_into(&mut config);
        detect_and_apply_terminal(&mut config);

        info!("Loaded Lua config from {:?}", lua_path);

        Ok(Self {
            config,
            lua: Some(ctx),
            lua_path: Some(lua_path.to_path_buf()),
            user_providers: lua_cfg.user_providers,
        })
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

#[cfg(test)]
mod v2_rename_tests {
    use super::*;

    #[test]
    fn providers_config_accepts_power_key() {
        let toml = r#"[providers]
power = false
"#;
        let cfg: Config = toml::from_str(toml).expect("must parse");
        assert!(!cfg.providers.power);
    }

    #[test]
    fn providers_config_accepts_pre_v2_system_alias() {
        // Pre-v2 configs used `system = ...`. Serde alias keeps them working.
        let toml = r#"[providers]
system = false
"#;
        let cfg: Config = toml::from_str(toml).expect("must parse");
        assert!(!cfg.providers.power);
    }

    #[test]
    fn theme_colors_accepts_pre_v2_badge_sys_alias() {
        // Pre-v2 stylesheets named the color `badge_sys`. Serde alias keeps
        // existing user configs working.
        let toml = r##"[appearance.colors]
badge_sys = "#ff8800"
"##;
        let cfg: Config = toml::from_str(toml).expect("must parse");
        assert_eq!(
            cfg.appearance.colors.badge_power.as_deref(),
            Some("#ff8800")
        );
    }
}

#[cfg(all(test, feature = "lua"))]
mod loaded_config_tests {
    //! Tests for the Lua-aware config resolver. Each test writes a temp file
    //! and feeds the exact path through [`LoadedConfig::load_lua_path`] or
    //! [`Config::load_from_toml`] to avoid touching the real XDG config dir.

    use super::*;
    use std::io::Write;

    fn write_file(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(contents.as_bytes()).expect("write");
        path
    }

    #[test]
    fn defaults_apply_when_no_files_exist() {
        let lc = LoadedConfig::defaults();
        assert!(lc.lua.is_none());
        assert!(lc.lua_path.is_none());
        assert!(lc.user_providers.is_empty());
        // Terminal detection should always fill this in.
        assert!(lc.config.general.terminal_command.is_some());
    }

    #[test]
    fn lua_path_overrides_defaults_with_set_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lua = write_file(
            dir.path(),
            "owlry.lua",
            r#"owlry.set { theme = "nord", width = 1024, max_results = 25 }"#,
        );

        let lc = LoadedConfig::load_lua_path(&lua).expect("load");
        assert_eq!(lc.config.appearance.theme.as_deref(), Some("nord"));
        assert_eq!(lc.config.appearance.width, 1024);
        assert_eq!(lc.config.general.max_results, 25);
        assert!(lc.lua.is_some(), "LuaContext must be retained");
        assert_eq!(lc.lua_path.as_deref(), Some(lua.as_path()));
        assert!(lc.user_providers.is_empty());
    }

    #[test]
    fn lua_providers_list_disables_unlisted_built_ins() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lua = write_file(
            dir.path(),
            "owlry.lua",
            r#"owlry.providers { "app", "cmd" }"#, // power, ssh, emoji etc. → off
        );
        let lc = LoadedConfig::load_lua_path(&lua).expect("load");
        assert!(lc.config.providers.applications);
        assert!(lc.config.providers.commands);
        assert!(!lc.config.providers.power);
        assert!(!lc.config.providers.ssh);
        assert!(!lc.config.providers.systemd);
    }

    #[test]
    fn lua_user_provider_is_captured() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lua = write_file(
            dir.path(),
            "owlry.lua",
            r#"
            owlry.provider {
              id = "hs",
              prefix = ":hs",
              items = function() return { { name = "Lock", command = "hyprlock" } } end,
            }
            "#,
        );
        let lc = LoadedConfig::load_lua_path(&lua).expect("load");
        assert_eq!(lc.user_providers.len(), 1);
        assert_eq!(lc.user_providers[0].id, "hs");
        assert_eq!(lc.user_providers[0].prefix.as_deref(), Some(":hs"));
    }

    #[test]
    fn lua_provider_remains_callable_after_load() {
        // End-to-end: a LuaProvider built from a LoadedConfig must still be
        // able to call its items function once the spec is wired up. This
        // is the integration that 3.4 unlocks.
        use crate::lua::LuaProvider;
        use crate::providers::Provider;

        let dir = tempfile::tempdir().expect("tempdir");
        let lua = write_file(
            dir.path(),
            "owlry.lua",
            r#"
            owlry.provider {
              id = "hs",
              items = function()
                return {
                  { name = "Lock",     command = "hyprlock" },
                  { name = "Shutdown", command = "systemctl poweroff" },
                }
              end,
            }
            "#,
        );
        let lc = LoadedConfig::load_lua_path(&lua).expect("load");
        let ctx = lc.lua.as_ref().expect("lua context");
        let mut p = LuaProvider::new(lc.user_providers[0].clone(), ctx.lua_handle());
        p.refresh();
        let items = p.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Lock");
        assert_eq!(items[1].name, "Shutdown");
    }

    #[test]
    fn lua_syntax_error_is_surfaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lua = write_file(dir.path(), "owlry.lua", "this is not valid lua $$$");
        let err = LoadedConfig::load_lua_path(&lua).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("Lua") || msg.contains("evaluation"),
            "should mention Lua failure; got: {}",
            msg
        );
    }

    #[test]
    fn toml_only_load_returns_defaults_when_path_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist.toml");
        let cfg = Config::load_from_toml(Some(&missing)).expect("load");
        // Defaults — terminal NOT auto-detected here (helper is internal).
        assert!(cfg.providers.applications);
        assert!(cfg.providers.power);
    }

    #[test]
    fn toml_only_load_parses_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml = write_file(
            dir.path(),
            "config.toml",
            r#"
[providers]
power = false
ssh = false
"#,
        );
        let cfg = Config::load_from_toml(Some(&toml)).expect("load");
        assert!(!cfg.providers.power);
        assert!(!cfg.providers.ssh);
        // Untouched keys keep their defaults.
        assert!(cfg.providers.applications);
    }
}
