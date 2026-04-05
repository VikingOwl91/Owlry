use std::sync::{Arc, RwLock};

use log::warn;

use super::{DynamicProvider, LaunchItem, ProviderType};
use crate::config::Config;

const ICON: &str = "preferences-system-symbolic";
const PROVIDER_TYPE_ID: &str = "config";

/// Known search engines for the engine selection list.
const SEARCH_ENGINES: &[&str] = &[
    "duckduckgo",
    "google",
    "bing",
    "startpage",
    "searxng",
    "brave",
    "ecosia",
];

/// Known built-in theme names (always available).
const BUILTIN_THEMES: &[&str] = &["owl"];

/// Boolean provider fields that can be toggled via CONFIG:toggle:providers.*.
/// Only built-in providers are listed here; plugins are enabled/disabled via
/// [plugins] disabled_plugins in config.toml or `owlry plugin enable/disable`.
const PROVIDER_TOGGLES: &[(&str, &str)] = &[
    ("applications", "Applications"),
    ("commands", "Commands"),
    ("calculator", "Calculator"),
    ("converter", "Unit Converter"),
    ("system", "System Actions"),
    ("frecency", "Frecency Ranking"),
];

/// Built-in config editor provider. Interprets query text as a navigation path
/// and generates settings items the user can activate to change configuration.
pub(crate) struct ConfigProvider {
    config: Arc<RwLock<Config>>,
}

impl ConfigProvider {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self { config }
    }

    /// Execute a `CONFIG:*` action command. Returns `true` if handled.
    fn handle_config_action(&self, command: &str) -> bool {
        let Some(rest) = command.strip_prefix("CONFIG:") else {
            return false;
        };

        let result = if let Some(path) = rest.strip_prefix("toggle:") {
            self.handle_toggle(path)
        } else if let Some(kv) = rest.strip_prefix("set:") {
            self.handle_set(kv)
        } else if let Some(profile_cmd) = rest.strip_prefix("profile:") {
            self.handle_profile(profile_cmd)
        } else {
            false
        };

        if result && let Ok(cfg) = self.config.read() && let Err(e) = cfg.save() {
            warn!("Failed to save config: {}", e);
        }

        result
    }

    // ── Toggle handler ──────────────────────────────────────────────────

    fn handle_toggle(&self, path: &str) -> bool {
        let mut cfg = match self.config.write() {
            Ok(c) => c,
            Err(_) => return false,
        };

        match path {
            "providers.applications" => {
                cfg.providers.applications = !cfg.providers.applications;
                true
            }
            "providers.commands" => {
                cfg.providers.commands = !cfg.providers.commands;
                true
            }
            "providers.calculator" => {
                cfg.providers.calculator = !cfg.providers.calculator;
                true
            }
            "providers.converter" => {
                cfg.providers.converter = !cfg.providers.converter;
                true
            }
            "providers.system" => {
                cfg.providers.system = !cfg.providers.system;
                true
            }
            "providers.frecency" => {
                cfg.providers.frecency = !cfg.providers.frecency;
                true
            }
            "general.show_icons" => {
                cfg.general.show_icons = !cfg.general.show_icons;
                true
            }
            "general.use_uwsm" => {
                cfg.general.use_uwsm = !cfg.general.use_uwsm;
                true
            }
            _ => false,
        }
    }

    // ── Set handler ─────────────────────────────────────────────────────

    fn handle_set(&self, kv: &str) -> bool {
        let Some((path, value)) = kv.split_once(':') else {
            return false;
        };
        let mut cfg = match self.config.write() {
            Ok(c) => c,
            Err(_) => return false,
        };

        match path {
            "appearance.theme" => {
                cfg.appearance.theme = if value == "none" {
                    None
                } else {
                    Some(value.to_string())
                };
                true
            }
            "appearance.font_size" => {
                if let Ok(v) = value.parse::<u32>() {
                    cfg.appearance.font_size = v;
                    true
                } else {
                    false
                }
            }
            "appearance.width" => {
                if let Ok(v) = value.parse::<i32>() {
                    cfg.appearance.width = v;
                    true
                } else {
                    false
                }
            }
            "appearance.height" => {
                if let Ok(v) = value.parse::<i32>() {
                    cfg.appearance.height = v;
                    true
                } else {
                    false
                }
            }
            "appearance.border_radius" => {
                if let Ok(v) = value.parse::<u32>() {
                    cfg.appearance.border_radius = v;
                    true
                } else {
                    false
                }
            }
            "providers.search_engine" => {
                cfg.providers.search_engine = value.to_string();
                true
            }
            "providers.frecency_weight" => {
                if let Ok(v) = value.parse::<f64>() {
                    cfg.providers.frecency_weight = v.clamp(0.0, 1.0);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    // ── Profile handler ─────────────────────────────────────────────────

    fn handle_profile(&self, cmd: &str) -> bool {
        if let Some(name) = cmd.strip_prefix("create:") {
            let mut cfg = match self.config.write() {
                Ok(c) => c,
                Err(_) => return false,
            };
            if !name.is_empty() && !cfg.profiles.contains_key(name) {
                cfg.profiles.insert(
                    name.to_string(),
                    crate::config::ProfileConfig::default(),
                );
                true
            } else {
                false
            }
        } else if let Some(name) = cmd.strip_prefix("delete:") {
            let mut cfg = match self.config.write() {
                Ok(c) => c,
                Err(_) => return false,
            };
            cfg.profiles.remove(name).is_some()
        } else if let Some(rest) = cmd.strip_prefix("mode:") {
            // format: profile_name:toggle:mode_name
            let parts: Vec<&str> = rest.splitn(3, ':').collect();
            if parts.len() == 3 && parts[1] == "toggle" {
                let profile_name = parts[0];
                let mode_name = parts[2];
                let mut cfg = match self.config.write() {
                    Ok(c) => c,
                    Err(_) => return false,
                };
                if let Some(profile) = cfg.profiles.get_mut(profile_name) {
                    if let Some(pos) = profile.modes.iter().position(|m| m == mode_name) {
                        profile.modes.remove(pos);
                    } else {
                        profile.modes.push(mode_name.to_string());
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    // ── Query routing ───────────────────────────────────────────────────

    fn query_top_level(&self) -> Vec<LaunchItem> {
        vec![
            nav_item("config:cat:providers", "Providers", "Toggle search providers on/off"),
            nav_item("config:cat:theme", "Theme", "Select UI theme"),
            nav_item("config:cat:engine", "Engine", "Search engine for web search"),
            nav_item("config:cat:frecency", "Frecency", "Frecency ranking settings"),
            nav_item("config:cat:fontsize", "Font Size", "Adjust font size"),
            nav_item("config:cat:width", "Width", "Adjust window width"),
            nav_item("config:cat:height", "Height", "Adjust window height"),
            nav_item("config:cat:radius", "Border Radius", "Adjust border radius"),
            nav_item("config:cat:profiles", "Profiles", "Manage provider profiles"),
        ]
    }

    fn query_providers(&self, filter: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        PROVIDER_TOGGLES
            .iter()
            .filter(|(_, label)| {
                filter.is_empty() || label.to_lowercase().contains(&filter.to_lowercase())
            })
            .map(|(field, label)| {
                let enabled = get_provider_bool(&cfg, field);
                let marker = if enabled { "\u{2713}" } else { "\u{2717}" };
                let status = if enabled { "enabled" } else { "disabled" };
                LaunchItem {
                    id: format!("config:toggle:providers.{}", field),
                    name: format!("{} {}", marker, label),
                    description: Some(format!("Currently {}", status)),
                    icon: Some(ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:toggle:providers.{}", field),
                    terminal: false,
                    tags: vec!["config".into(), "settings".into()],
                }
            })
            .collect()
    }

    fn query_theme(&self, filter: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let current = cfg.appearance.theme.as_deref().unwrap_or("(GTK default)");

        let mut themes: Vec<&str> = Vec::new();
        themes.push("none"); // GTK default
        for t in BUILTIN_THEMES {
            themes.push(t);
        }

        // Discover user themes from filesystem
        let user_themes = discover_user_themes();
        let user_theme_refs: Vec<&str> = user_themes.iter().map(|s| s.as_str()).collect();
        themes.extend_from_slice(&user_theme_refs);

        themes
            .into_iter()
            .filter(|t| {
                if filter.is_empty() {
                    true
                } else {
                    t.to_lowercase().contains(&filter.to_lowercase())
                }
            })
            .map(|theme_name| {
                let display = if theme_name == "none" {
                    "GTK Default".to_string()
                } else {
                    theme_name.to_string()
                };
                let is_current = if theme_name == "none" {
                    cfg.appearance.theme.is_none()
                } else {
                    cfg.appearance.theme.as_deref() == Some(theme_name)
                };
                let marker = if is_current { "\u{25cf} " } else { "  " };
                LaunchItem {
                    id: format!("config:theme:{}", theme_name),
                    name: format!("{}{}", marker, display),
                    description: Some(format!("Current: {}", current)),
                    icon: Some(ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:appearance.theme:{}", theme_name),
                    terminal: false,
                    tags: vec!["config".into(), "settings".into()],
                }
            })
            .collect()
    }

    fn query_engine(&self, filter: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let current = &cfg.providers.search_engine;

        SEARCH_ENGINES
            .iter()
            .filter(|e| {
                filter.is_empty() || e.to_lowercase().contains(&filter.to_lowercase())
            })
            .map(|engine| {
                let is_current = *engine == current.as_str();
                let marker = if is_current { "\u{25cf} " } else { "  " };
                LaunchItem {
                    id: format!("config:engine:{}", engine),
                    name: format!("{}{}", marker, engine),
                    description: Some(format!("Current: {}", current)),
                    icon: Some(ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:providers.search_engine:{}", engine),
                    terminal: false,
                    tags: vec!["config".into(), "settings".into()],
                }
            })
            .collect()
    }

    fn query_frecency(&self, input: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut items = Vec::new();

        // Toggle item
        let enabled = cfg.providers.frecency;
        let marker = if enabled { "\u{2713}" } else { "\u{2717}" };
        let status = if enabled { "enabled" } else { "disabled" };
        items.push(LaunchItem {
            id: "config:toggle:providers.frecency".into(),
            name: format!("{} Frecency Ranking", marker),
            description: Some(format!("Currently {} (weight: {})", status, cfg.providers.frecency_weight)),
            icon: Some(ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: "CONFIG:toggle:providers.frecency".into(),
            terminal: false,
            tags: vec!["config".into(), "settings".into()],
        });

        // If numeric input, offer a set-weight action
        if let Ok(weight) = input.parse::<f64>() {
            let clamped = weight.clamp(0.0, 1.0);
            items.push(LaunchItem {
                id: format!("config:set:frecency_weight:{}", clamped),
                name: format!("Set weight to {}", clamped),
                description: Some(format!("Current: {}", cfg.providers.frecency_weight)),
                icon: Some(ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: format!("CONFIG:set:providers.frecency_weight:{}", clamped),
                terminal: false,
                tags: vec!["config".into(), "settings".into()],
            });
        }

        items
    }

    fn query_numeric(
        &self,
        category: &str,
        config_path: &str,
        label: &str,
        current_value: &str,
        input: &str,
    ) -> Vec<LaunchItem> {
        let mut items = Vec::new();

        // Show current value
        items.push(LaunchItem {
            id: format!("config:current:{}", category),
            name: format!("{}: {}", label, current_value),
            description: Some("Type a number to change".into()),
            icon: Some(ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: String::new(),
            terminal: false,
            tags: vec!["config".into(), "settings".into()],
        });

        // If numeric input, offer a set action
        if !input.is_empty() {
            // Validate it parses as the expected type
            let valid = input.parse::<f64>().is_ok();
            if valid {
                items.push(LaunchItem {
                    id: format!("config:set:{}:{}", category, input),
                    name: format!("Set {} to {}", label, input),
                    description: Some(format!("Current: {}", current_value)),
                    icon: Some(ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:{}:{}", config_path, input),
                    terminal: false,
                    tags: vec!["config".into(), "settings".into()],
                });
            }
        }

        items
    }

    fn query_profiles(&self, filter: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut items = Vec::new();

        for (name, profile) in &cfg.profiles {
            if !filter.is_empty() && !name.to_lowercase().contains(&filter.to_lowercase()) {
                continue;
            }
            let modes = profile.modes.join(", ");
            let desc = if modes.is_empty() {
                "No modes configured".to_string()
            } else {
                format!("Modes: {}", modes)
            };
            items.push(nav_item(
                &format!("config:profile:{}", name),
                name,
                &desc,
            ));
        }

        // Hint for creating
        if filter.is_empty() {
            items.push(LaunchItem {
                id: "config:profile:create:hint".into(),
                name: "Type a name to create a new profile".into(),
                description: None,
                icon: Some(ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: String::new(),
                terminal: false,
                tags: vec!["config".into(), "settings".into()],
            });
        }

        items
    }

    fn query_profile_create(&self, name: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        if cfg.profiles.contains_key(name) {
            return Vec::new();
        }

        vec![LaunchItem {
            id: format!("config:profile:create:{}", name),
            name: format!("Create profile '{}'", name),
            description: Some("Creates an empty profile".into()),
            icon: Some(ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: format!("CONFIG:profile:create:{}", name),
            terminal: false,
            tags: vec!["config".into(), "settings".into()],
        }]
    }

    fn query_profile_detail(&self, profile_name: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        if !cfg.profiles.contains_key(profile_name) {
            return Vec::new();
        }

        vec![
            nav_item(
                &format!("config:profile:{}:modes", profile_name),
                "Edit Modes",
                "Toggle which modes are included",
            ),
            LaunchItem {
                id: format!("config:profile:delete:{}", profile_name),
                name: format!("Delete profile '{}'", profile_name),
                description: None,
                icon: Some(ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: format!("CONFIG:profile:delete:{}", profile_name),
                terminal: false,
                tags: vec!["config".into(), "settings".into()],
            },
        ]
    }

    fn query_profile_modes(&self, profile_name: &str) -> Vec<LaunchItem> {
        let cfg = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let Some(profile) = cfg.profiles.get(profile_name) else {
            return Vec::new();
        };

        let all_modes = [
            "app", "cmd", "dmenu", "calc", "clip", "emoji", "ssh", "sys", "bm", "file", "web",
            "uuctl",
        ];

        all_modes
            .iter()
            .map(|mode| {
                let active = profile.modes.iter().any(|m| m == mode);
                let marker = if active { "\u{2713}" } else { "\u{2717}" };
                LaunchItem {
                    id: format!("config:profile:{}:mode:{}", profile_name, mode),
                    name: format!("{} {}", marker, mode),
                    description: None,
                    icon: Some(ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!(
                        "CONFIG:profile:mode:{}:toggle:{}",
                        profile_name, mode
                    ),
                    terminal: false,
                    tags: vec!["config".into(), "settings".into()],
                }
            })
            .collect()
    }
}

impl DynamicProvider for ConfigProvider {
    fn name(&self) -> &str {
        "Config Editor"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(PROVIDER_TYPE_ID.into())
    }

    fn priority(&self) -> u32 {
        8000
    }

    fn execute_action(&self, command: &str) -> bool {
        self.handle_config_action(command)
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        let q = query.trim().to_lowercase();

        if q.is_empty() {
            return self.query_top_level();
        }

        // Route based on first path segment
        let (segment, rest) = match q.find(' ') {
            Some(pos) => (&q[..pos], q[pos + 1..].trim()),
            None => (q.as_str(), ""),
        };

        match segment {
            "providers" => self.query_providers(rest),
            "theme" => self.query_theme(rest),
            "engine" => self.query_engine(rest),
            "frecency" => self.query_frecency(rest),
            "fontsize" => {
                let cfg = self.config.read().ok();
                let current = cfg
                    .as_ref()
                    .map(|c| c.appearance.font_size.to_string())
                    .unwrap_or_default();
                self.query_numeric("fontsize", "appearance.font_size", "Font Size", &current, rest)
            }
            "width" => {
                let cfg = self.config.read().ok();
                let current = cfg
                    .as_ref()
                    .map(|c| c.appearance.width.to_string())
                    .unwrap_or_default();
                self.query_numeric("width", "appearance.width", "Width", &current, rest)
            }
            "height" => {
                let cfg = self.config.read().ok();
                let current = cfg
                    .as_ref()
                    .map(|c| c.appearance.height.to_string())
                    .unwrap_or_default();
                self.query_numeric("height", "appearance.height", "Height", &current, rest)
            }
            "radius" => {
                let cfg = self.config.read().ok();
                let current = cfg
                    .as_ref()
                    .map(|c| c.appearance.border_radius.to_string())
                    .unwrap_or_default();
                self.query_numeric(
                    "radius",
                    "appearance.border_radius",
                    "Border Radius",
                    &current,
                    rest,
                )
            }
            "profiles" => self.query_profiles(rest),
            "profile" => {
                if rest.is_empty() {
                    self.query_profiles("")
                } else {
                    let (profile_name, sub) = match rest.find(' ') {
                        Some(pos) => (&rest[..pos], rest[pos + 1..].trim()),
                        None => (rest, ""),
                    };
                    if profile_name == "create" {
                        if sub.is_empty() {
                            Vec::new()
                        } else {
                            self.query_profile_create(sub)
                        }
                    } else if sub.is_empty() {
                        self.query_profile_detail(profile_name)
                    } else if sub == "modes" {
                        self.query_profile_modes(profile_name)
                    } else {
                        Vec::new()
                    }
                }
            }
            _ => {
                // Fuzzy filter top-level categories
                self.query_top_level()
                    .into_iter()
                    .filter(|item| item.name.to_lowercase().contains(&q))
                    .collect()
            }
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Create a navigation item (no action command — selecting it refines the query).
fn nav_item(id: &str, name: &str, description: &str) -> LaunchItem {
    LaunchItem {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        icon: Some(ICON.into()),
        provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
        command: String::new(),
        terminal: false,
        tags: vec!["config".into(), "settings".into()],
    }
}

/// Read a boolean field from ProvidersConfig by field name.
fn get_provider_bool(cfg: &Config, field: &str) -> bool {
    match field {
        "applications" => cfg.providers.applications,
        "commands" => cfg.providers.commands,
        "calculator" => cfg.providers.calculator,
        "converter" => cfg.providers.converter,
        "system" => cfg.providers.system,
        "frecency" => cfg.providers.frecency,
        _ => false,
    }
}

/// Discover user theme CSS files from the themes directory.
fn discover_user_themes() -> Vec<String> {
    let Some(dir) = crate::paths::themes_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut themes: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("css") {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        // Exclude the built-in theme name to avoid duplicates
        .filter(|name| !BUILTIN_THEMES.contains(&name.as_str()))
        .collect();

    themes.sort();
    themes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    fn make_provider() -> ConfigProvider {
        ConfigProvider::new(Arc::new(RwLock::new(Config::default())))
    }

    fn make_provider_with_config(config: Config) -> ConfigProvider {
        ConfigProvider::new(Arc::new(RwLock::new(config)))
    }

    // ── Top-level ───────────────────────────────────────────────────────

    #[test]
    fn empty_query_returns_top_level_categories() {
        let p = make_provider();
        let results = p.query("");
        assert!(results.len() >= 7, "expected at least 7 categories, got {}", results.len());
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Providers"));
        assert!(names.contains(&"Theme"));
        assert!(names.contains(&"Engine"));
        assert!(names.contains(&"Frecency"));
        assert!(names.contains(&"Font Size"));
        assert!(names.contains(&"Profiles"));
    }

    // ── Provider toggles ────────────────────────────────────────────────

    #[test]
    fn providers_query_shows_toggle_list() {
        let p = make_provider();
        let results = p.query("providers");
        assert!(!results.is_empty());
        // Default config has all providers enabled
        assert!(results[0].name.contains('\u{2713}'));
        assert!(results[0].command.starts_with("CONFIG:toggle:providers."));
    }

    #[test]
    fn toggle_action_flips_boolean_and_back() {
        let p = make_provider();

        // Calculator starts enabled
        assert!(p.config.read().unwrap().providers.calculator);

        // Toggle off
        assert!(p.execute_action("CONFIG:toggle:providers.calculator"));
        assert!(!p.config.read().unwrap().providers.calculator);

        // Toggle back on
        assert!(p.execute_action("CONFIG:toggle:providers.calculator"));
        assert!(p.config.read().unwrap().providers.calculator);
    }

    // ── Theme ───────────────────────────────────────────────────────────

    #[test]
    fn theme_items_mark_current_with_bullet() {
        let p = make_provider();
        let results = p.query("theme");
        // Default config has no theme set -> GTK Default is current
        let gtk_item = results.iter().find(|r| r.name.contains("GTK Default")).unwrap();
        assert!(gtk_item.name.starts_with("\u{25cf}"), "expected bullet marker on current theme");
    }

    #[test]
    fn set_theme_action_updates_config() {
        let p = make_provider();

        assert!(p.execute_action("CONFIG:set:appearance.theme:owl"));
        assert_eq!(
            p.config.read().unwrap().appearance.theme.as_deref(),
            Some("owl")
        );
    }

    #[test]
    fn set_theme_to_none_clears_theme() {
        let mut cfg = Config::default();
        cfg.appearance.theme = Some("owl".into());
        let p = make_provider_with_config(cfg);

        assert!(p.execute_action("CONFIG:set:appearance.theme:none"));
        assert!(p.config.read().unwrap().appearance.theme.is_none());
    }

    // ── Numeric values ──────────────────────────────────────────────────

    #[test]
    fn set_numeric_font_size() {
        let p = make_provider();

        assert!(p.execute_action("CONFIG:set:appearance.font_size:16"));
        assert_eq!(p.config.read().unwrap().appearance.font_size, 16);
    }

    #[test]
    fn fontsize_query_with_number_generates_set_action() {
        let p = make_provider();
        let results = p.query("fontsize 16");
        assert!(results.len() >= 2, "expected current + set action items");
        let set_item = results.iter().find(|r| r.name.contains("Set")).unwrap();
        assert_eq!(set_item.command, "CONFIG:set:appearance.font_size:16");
    }

    #[test]
    fn set_width_works() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:set:appearance.width:900"));
        assert_eq!(p.config.read().unwrap().appearance.width, 900);
    }

    #[test]
    fn set_height_works() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:set:appearance.height:700"));
        assert_eq!(p.config.read().unwrap().appearance.height, 700);
    }

    #[test]
    fn set_border_radius_works() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:set:appearance.border_radius:8"));
        assert_eq!(p.config.read().unwrap().appearance.border_radius, 8);
    }

    // ── Frecency ────────────────────────────────────────────────────────

    #[test]
    fn frecency_weight_clamped_to_range() {
        let p = make_provider();

        // Above 1.0 clamped to 1.0
        assert!(p.execute_action("CONFIG:set:providers.frecency_weight:2.5"));
        assert_eq!(p.config.read().unwrap().providers.frecency_weight, 1.0);

        // Below 0.0 clamped to 0.0
        assert!(p.execute_action("CONFIG:set:providers.frecency_weight:-0.5"));
        assert_eq!(p.config.read().unwrap().providers.frecency_weight, 0.0);

        // Normal value accepted
        assert!(p.execute_action("CONFIG:set:providers.frecency_weight:0.5"));
        assert_eq!(p.config.read().unwrap().providers.frecency_weight, 0.5);
    }

    #[test]
    fn frecency_query_shows_toggle_and_weight() {
        let p = make_provider();
        let results = p.query("frecency");
        assert!(!results.is_empty());
        assert!(results[0].name.contains("Frecency Ranking"));
        assert!(results[0].description.as_ref().unwrap().contains("weight"));
    }

    #[test]
    fn frecency_numeric_input_generates_set_action() {
        let p = make_provider();
        let results = p.query("frecency 0.5");
        let set_item = results.iter().find(|r| r.name.contains("Set weight")).unwrap();
        assert_eq!(set_item.command, "CONFIG:set:providers.frecency_weight:0.5");
    }

    // ── Engine ──────────────────────────────────────────────────────────

    #[test]
    fn engine_query_lists_engines_with_current_marker() {
        let p = make_provider();
        let results = p.query("engine");
        assert!(!results.is_empty());
        // Default is duckduckgo — should have bullet marker
        let ddg = results.iter().find(|r| r.name.contains("duckduckgo")).unwrap();
        assert!(ddg.name.contains("\u{25cf}"));
    }

    #[test]
    fn set_engine_updates_config() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:set:providers.search_engine:google"));
        assert_eq!(p.config.read().unwrap().providers.search_engine, "google");
    }

    // ── Invalid actions ─────────────────────────────────────────────────

    #[test]
    fn invalid_action_returns_false() {
        let p = make_provider();
        assert!(!p.execute_action("CONFIG:toggle:nonexistent.field"));
        assert!(!p.execute_action("CONFIG:set:nonexistent.field:value"));
        assert!(!p.execute_action("NOTCONFIG:something"));
        assert!(!p.execute_action("CONFIG:unknown_verb:something"));
    }

    // ── Provider type ───────────────────────────────────────────────────

    #[test]
    fn provider_type_is_plugin_config() {
        let p = make_provider();
        assert_eq!(
            p.provider_type(),
            ProviderType::Plugin("config".into())
        );
    }

    #[test]
    fn provider_priority_is_8000() {
        let p = make_provider();
        assert_eq!(p.priority(), 8000);
    }

    // ── Profiles ────────────────────────────────────────────────────────

    #[test]
    fn profile_create_action_adds_profile() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:profile:create:dev"));
        assert!(p.config.read().unwrap().profiles.contains_key("dev"));
    }

    #[test]
    fn profile_create_duplicate_returns_false() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:profile:create:dev"));
        assert!(!p.execute_action("CONFIG:profile:create:dev"));
    }

    #[test]
    fn profile_delete_action_removes_profile() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:profile:create:dev"));
        assert!(p.execute_action("CONFIG:profile:delete:dev"));
        assert!(!p.config.read().unwrap().profiles.contains_key("dev"));
    }

    #[test]
    fn profile_delete_nonexistent_returns_false() {
        let p = make_provider();
        assert!(!p.execute_action("CONFIG:profile:delete:nonexistent"));
    }

    #[test]
    fn profile_mode_toggle_adds_and_removes() {
        let p = make_provider();
        assert!(p.execute_action("CONFIG:profile:create:dev"));

        // Add mode
        assert!(p.execute_action("CONFIG:profile:mode:dev:toggle:ssh"));
        assert!(
            p.config
                .read()
                .unwrap()
                .profiles
                .get("dev")
                .unwrap()
                .modes
                .contains(&"ssh".to_string())
        );

        // Remove mode
        assert!(p.execute_action("CONFIG:profile:mode:dev:toggle:ssh"));
        assert!(
            !p.config
                .read()
                .unwrap()
                .profiles
                .get("dev")
                .unwrap()
                .modes
                .contains(&"ssh".to_string())
        );
    }

    #[test]
    fn profile_mode_toggle_nonexistent_profile_returns_false() {
        let p = make_provider();
        assert!(!p.execute_action("CONFIG:profile:mode:nonexistent:toggle:ssh"));
    }

    #[test]
    fn profile_create_query_generates_correct_action_item() {
        let p = make_provider();
        let results = p.query("profile create dev");
        assert_eq!(results.len(), 1);
        assert!(results[0].name.contains("Create profile 'dev'"));
        assert_eq!(results[0].command, "CONFIG:profile:create:dev");
    }

    #[test]
    fn profiles_query_lists_existing_profiles() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "dev".into(),
            crate::config::ProfileConfig {
                modes: vec!["app".into(), "cmd".into()],
            },
        );
        let p = make_provider_with_config(cfg);

        let results = p.query("profiles");
        assert!(results.iter().any(|r| r.name == "dev"));
    }

    #[test]
    fn profile_detail_shows_edit_and_delete() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "dev".into(),
            crate::config::ProfileConfig {
                modes: vec!["app".into()],
            },
        );
        let p = make_provider_with_config(cfg);

        let results = p.query("profile dev");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Edit Modes"));
        assert!(names.iter().any(|n| n.contains("Delete")));
    }

    #[test]
    fn profile_modes_shows_checklist() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "dev".into(),
            crate::config::ProfileConfig {
                modes: vec!["app".into(), "ssh".into()],
            },
        );
        let p = make_provider_with_config(cfg);

        let results = p.query("profile dev modes");
        assert!(!results.is_empty());

        // app and ssh should be checked
        let app_item = results.iter().find(|r| r.name.contains("app")).unwrap();
        assert!(app_item.name.contains('\u{2713}'));

        let cmd_item = results.iter().find(|r| r.name.contains("cmd")).unwrap();
        assert!(cmd_item.name.contains('\u{2717}'));
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn navigation_items_have_empty_command() {
        let p = make_provider();
        let results = p.query("");
        for item in &results {
            assert!(item.command.is_empty(), "nav item '{}' should have empty command", item.name);
        }
    }

    #[test]
    fn all_items_have_config_tags() {
        let p = make_provider();
        let results = p.query("providers");
        for item in &results {
            assert!(item.tags.contains(&"config".into()));
            assert!(item.tags.contains(&"settings".into()));
        }
    }

    #[test]
    fn provider_filter_narrows_results() {
        let p = make_provider();
        let all = p.query("providers");
        let filtered = p.query("providers calc");
        assert!(filtered.len() < all.len());
        assert!(filtered.iter().all(|r| r.name.to_lowercase().contains("calc")));
    }

    #[test]
    fn theme_filter_narrows_results() {
        let p = make_provider();
        let results = p.query("theme owl");
        assert!(results.iter().any(|r| r.name.to_lowercase().contains("owl")));
    }
}
