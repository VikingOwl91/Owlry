# Config Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Built-in `:config` provider that lets users browse and modify settings, toggle providers, select themes/engines, and manage profiles — all from within the launcher.

**Architecture:** The config editor is a `DynamicProvider` that interprets the query text as a navigation path. `:config providers` shows toggles, `:config theme` lists themes, `:config profile dev modes` shows a mode checklist. Actions (toggling, setting values) use the existing `PluginAction` IPC flow which keeps the window open and re-queries, giving instant visual feedback. Config changes are persisted to `config.toml` via `Config::save()`.

**Tech Stack:** Rust, owlry-core providers, toml serialization

---

## Key Design Decision: Query-as-Navigation

Instead of submenus, the `:config` prefix scopes the search bar as navigation:

```
:config                          → category list
:config providers                → provider toggles
:config theme                    → theme selection
:config engine                   → search engine selection
:config frecency                 → frecency toggle + weight
:config profiles                 → profile list
:config profile dev              → profile actions (edit modes, rename, delete)
:config profile dev modes        → mode checklist for profile
:config profile create myname    → create profile action
:config fontsize 16              → set font size action
:config width 900                → set width action
```

Actions use `CONFIG:*` commands dispatched via `execute_plugin_action`. Since this returns `false` for `should_close`, the window stays open and re-queries — the user sees updated state immediately.

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/owlry-core/src/providers/config_editor.rs` | Create | ConfigProvider: query parsing, result generation, action execution |
| `crates/owlry-core/src/providers/mod.rs` | Modify | Register ConfigProvider, extend action dispatch |
| `crates/owlry-core/src/config/mod.rs` | Modify | Add helper methods for config mutation |

---

### Task 1: Create ConfigProvider skeleton and register it

**Files:**
- Create: `crates/owlry-core/src/providers/config_editor.rs`
- Modify: `crates/owlry-core/src/providers/mod.rs`

- [ ] **Step 1: Add module declaration**

In `crates/owlry-core/src/providers/mod.rs`, add with the other module declarations:

```rust
pub(crate) mod config_editor;
```

- [ ] **Step 2: Create config_editor.rs with top-level categories**

Create `crates/owlry-core/src/providers/config_editor.rs`:

```rust
//! Built-in config editor provider.
//!
//! Lets users browse and modify settings from within the launcher.
//! Uses `:config` prefix with query-as-navigation pattern.

use std::sync::{Arc, RwLock};

use crate::config::Config;
use super::{DynamicProvider, LaunchItem, ProviderType};

const PROVIDER_TYPE_ID: &str = "config";
const PROVIDER_ICON: &str = "preferences-system-symbolic";

pub struct ConfigProvider {
    config: Arc<RwLock<Config>>,
}

impl ConfigProvider {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self { config }
    }

    /// Execute a CONFIG:* action command. Returns true if handled.
    pub fn execute_action(&self, command: &str) -> bool {
        let Some(action) = command.strip_prefix("CONFIG:") else {
            return false;
        };
        let mut config = match self.config.write() {
            Ok(c) => c,
            Err(_) => return false,
        };

        let handled = self.handle_action(action, &mut config);

        if handled {
            if let Err(e) = config.save() {
                log::warn!("Failed to save config: {}", e);
            }
        }

        handled
    }

    fn handle_action(&self, action: &str, config: &mut Config) -> bool {
        if let Some(key) = action.strip_prefix("toggle:") {
            return self.toggle_bool(key, config);
        }
        if let Some(rest) = action.strip_prefix("set:") {
            return self.set_value(rest, config);
        }
        if let Some(rest) = action.strip_prefix("profile:") {
            return self.handle_profile_action(rest, config);
        }
        false
    }

    fn toggle_bool(&self, key: &str, config: &mut Config) -> bool {
        match key {
            "providers.applications" => { config.providers.applications = !config.providers.applications; true }
            "providers.commands" => { config.providers.commands = !config.providers.commands; true }
            "providers.calculator" => { config.providers.calculator = !config.providers.calculator; true }
            "providers.converter" => { config.providers.converter = !config.providers.converter; true }
            "providers.system" => { config.providers.system = !config.providers.system; true }
            "providers.websearch" => { config.providers.websearch = !config.providers.websearch; true }
            "providers.ssh" => { config.providers.ssh = !config.providers.ssh; true }
            "providers.clipboard" => { config.providers.clipboard = !config.providers.clipboard; true }
            "providers.bookmarks" => { config.providers.bookmarks = !config.providers.bookmarks; true }
            "providers.emoji" => { config.providers.emoji = !config.providers.emoji; true }
            "providers.scripts" => { config.providers.scripts = !config.providers.scripts; true }
            "providers.files" => { config.providers.files = !config.providers.files; true }
            "providers.uuctl" => { config.providers.uuctl = !config.providers.uuctl; true }
            "providers.media" => { config.providers.media = !config.providers.media; true }
            "providers.weather" => { config.providers.weather = !config.providers.weather; true }
            "providers.pomodoro" => { config.providers.pomodoro = !config.providers.pomodoro; true }
            "providers.frecency" => { config.providers.frecency = !config.providers.frecency; true }
            _ => false,
        }
    }

    fn set_value(&self, rest: &str, config: &mut Config) -> bool {
        let Some((key, value)) = rest.split_once(':') else { return false };
        match key {
            "appearance.theme" => { config.appearance.theme = Some(value.to_string()); true }
            "appearance.font_size" => {
                if let Ok(v) = value.parse::<i32>() {
                    config.appearance.font_size = v;
                    true
                } else { false }
            }
            "appearance.width" => {
                if let Ok(v) = value.parse::<i32>() {
                    config.appearance.width = v;
                    true
                } else { false }
            }
            "appearance.height" => {
                if let Ok(v) = value.parse::<i32>() {
                    config.appearance.height = v;
                    true
                } else { false }
            }
            "appearance.border_radius" => {
                if let Ok(v) = value.parse::<i32>() {
                    config.appearance.border_radius = v;
                    true
                } else { false }
            }
            "providers.search_engine" => { config.providers.search_engine = value.to_string(); true }
            "providers.frecency_weight" => {
                if let Ok(v) = value.parse::<f64>() {
                    config.providers.frecency_weight = v.clamp(0.0, 1.0);
                    true
                } else { false }
            }
            _ => false,
        }
    }

    fn handle_profile_action(&self, rest: &str, config: &mut Config) -> bool {
        if let Some(name) = rest.strip_prefix("create:") {
            config.profiles.entry(name.to_string()).or_insert_with(|| {
                crate::config::ProfileConfig { modes: vec![] }
            });
            return true;
        }
        if let Some(name) = rest.strip_prefix("delete:") {
            config.profiles.remove(name);
            return true;
        }
        if let Some(rest) = rest.strip_prefix("rename:") {
            if let Some((old, new)) = rest.split_once(':') {
                if let Some(profile) = config.profiles.remove(old) {
                    config.profiles.insert(new.to_string(), profile);
                    return true;
                }
            }
            return false;
        }
        if let Some(rest) = rest.strip_prefix("mode:") {
            // format: profile_name:toggle:mode_name
            let parts: Vec<&str> = rest.splitn(3, ':').collect();
            if parts.len() == 3 && parts[1] == "toggle" {
                let profile_name = parts[0];
                let mode = parts[2];
                if let Some(profile) = config.profiles.get_mut(profile_name) {
                    if let Some(pos) = profile.modes.iter().position(|m| m == mode) {
                        profile.modes.remove(pos);
                    } else {
                        profile.modes.push(mode.to_string());
                    }
                    return true;
                }
            }
            return false;
        }
        false
    }
}

impl DynamicProvider for ConfigProvider {
    fn name(&self) -> &str {
        "Config"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(PROVIDER_TYPE_ID.into())
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        let config = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let path = query.trim();
        self.generate_items(path, &config)
    }

    fn priority(&self) -> u32 {
        8_000
    }
}
```

- [ ] **Step 3: Implement generate_items — the query router**

Add to `ConfigProvider`:

```rust
    fn generate_items(&self, path: &str, config: &Config) -> Vec<LaunchItem> {
        // Top-level categories
        if path.is_empty() {
            return self.top_level_items();
        }

        let (section, rest) = match path.split_once(' ') {
            Some((s, r)) => (s, r.trim()),
            None => (path, ""),
        };

        match section {
            "providers" => self.provider_items(config),
            "theme" => self.theme_items(config, rest),
            "engine" => self.engine_items(config),
            "frecency" => self.frecency_items(config, rest),
            "fontsize" => self.numeric_item("Font Size", "appearance.font_size", config.appearance.font_size, rest),
            "width" => self.numeric_item("Width", "appearance.width", config.appearance.width, rest),
            "height" => self.numeric_item("Height", "appearance.height", config.appearance.height, rest),
            "radius" => self.numeric_item("Border Radius", "appearance.border_radius", config.appearance.border_radius, rest),
            "profiles" => self.profile_items(config, rest),
            "profile" => self.profile_detail_items(config, rest),
            _ => self.top_level_items(),
        }
    }

    fn top_level_items(&self) -> Vec<LaunchItem> {
        vec![
            self.make_item("config:providers", "Providers", "Toggle providers on/off", ""),
            self.make_item("config:theme", "Theme", "Select color theme", ""),
            self.make_item("config:engine", "Search Engine", "Select web search engine", ""),
            self.make_item("config:frecency", "Frecency", "Frecency ranking settings", ""),
            self.make_item("config:fontsize", "Font Size", "Set UI font size", ""),
            self.make_item("config:width", "Width", "Set window width", ""),
            self.make_item("config:height", "Height", "Set window height", ""),
            self.make_item("config:radius", "Border Radius", "Set border radius", ""),
            self.make_item("config:profiles", "Profiles", "Manage named mode profiles", ""),
        ]
    }

    fn make_item(&self, id: &str, name: &str, description: &str, command: &str) -> LaunchItem {
        LaunchItem {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(description.to_string()),
            icon: Some(PROVIDER_ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: command.to_string(),
            terminal: false,
            tags: vec!["config".into(), "settings".into()],
        }
    }

    fn toggle_item(&self, id: &str, name: &str, enabled: bool, key: &str) -> LaunchItem {
        let prefix = if enabled { "✓" } else { "✗" };
        LaunchItem {
            id: id.to_string(),
            name: format!("{} {}", prefix, name),
            description: Some(format!("{} (click to toggle)", if enabled { "Enabled" } else { "Disabled" })),
            icon: Some(PROVIDER_ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: format!("CONFIG:toggle:{}", key),
            terminal: false,
            tags: vec!["config".into()],
        }
    }
```

- [ ] **Step 4: Implement provider_items**

```rust
    fn provider_items(&self, config: &Config) -> Vec<LaunchItem> {
        vec![
            self.toggle_item("config:prov:app", "Applications", config.providers.applications, "providers.applications"),
            self.toggle_item("config:prov:cmd", "Commands", config.providers.commands, "providers.commands"),
            self.toggle_item("config:prov:calc", "Calculator", config.providers.calculator, "providers.calculator"),
            self.toggle_item("config:prov:conv", "Converter", config.providers.converter, "providers.converter"),
            self.toggle_item("config:prov:sys", "System", config.providers.system, "providers.system"),
            self.toggle_item("config:prov:web", "Web Search", config.providers.websearch, "providers.websearch"),
            self.toggle_item("config:prov:ssh", "SSH", config.providers.ssh, "providers.ssh"),
            self.toggle_item("config:prov:clip", "Clipboard", config.providers.clipboard, "providers.clipboard"),
            self.toggle_item("config:prov:bm", "Bookmarks", config.providers.bookmarks, "providers.bookmarks"),
            self.toggle_item("config:prov:emoji", "Emoji", config.providers.emoji, "providers.emoji"),
            self.toggle_item("config:prov:scripts", "Scripts", config.providers.scripts, "providers.scripts"),
            self.toggle_item("config:prov:files", "File Search", config.providers.files, "providers.files"),
            self.toggle_item("config:prov:uuctl", "systemd Units", config.providers.uuctl, "providers.uuctl"),
            self.toggle_item("config:prov:media", "Media", config.providers.media, "providers.media"),
            self.toggle_item("config:prov:weather", "Weather", config.providers.weather, "providers.weather"),
            self.toggle_item("config:prov:pomo", "Pomodoro", config.providers.pomodoro, "providers.pomodoro"),
        ]
    }
```

- [ ] **Step 5: Implement theme_items and engine_items**

```rust
    fn theme_items(&self, config: &Config, filter: &str) -> Vec<LaunchItem> {
        let current = config.appearance.theme.as_deref().unwrap_or("(default)");
        let themes = [
            "owl", "catppuccin-mocha", "nord", "rose-pine", "dracula",
            "gruvbox-dark", "tokyo-night", "solarized-dark", "one-dark", "apex-neon",
        ];

        themes.iter()
            .filter(|t| filter.is_empty() || t.contains(filter))
            .map(|t| {
                let mark = if *t == current { "● " } else { "  " };
                LaunchItem {
                    id: format!("config:theme:{}", t),
                    name: format!("{}{}", mark, t),
                    description: Some(if *t == current { "Current theme".into() } else { "Select this theme".into() }),
                    icon: Some(PROVIDER_ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:appearance.theme:{}", t),
                    terminal: false,
                    tags: vec!["config".into()],
                }
            })
            .collect()
    }

    fn engine_items(&self, config: &Config) -> Vec<LaunchItem> {
        let current = &config.providers.search_engine;
        let engines = [
            "duckduckgo", "google", "bing", "startpage", "brave", "ecosia",
        ];

        engines.iter()
            .map(|e| {
                let mark = if *e == current.as_str() { "● " } else { "  " };
                LaunchItem {
                    id: format!("config:engine:{}", e),
                    name: format!("{}{}", mark, e),
                    description: Some(if *e == current.as_str() { "Current engine".into() } else { "Select this engine".into() }),
                    icon: Some(PROVIDER_ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:providers.search_engine:{}", e),
                    terminal: false,
                    tags: vec!["config".into()],
                }
            })
            .collect()
    }
```

- [ ] **Step 6: Implement frecency_items and numeric_item**

```rust
    fn frecency_items(&self, config: &Config, rest: &str) -> Vec<LaunchItem> {
        let mut items = vec![
            self.toggle_item("config:frecency:toggle", "Frecency Ranking", config.providers.frecency, "providers.frecency"),
        ];

        // If user typed a weight value, show a set action
        if !rest.is_empty() {
            if let Ok(v) = rest.parse::<f64>() {
                let clamped = v.clamp(0.0, 1.0);
                items.push(LaunchItem {
                    id: "config:frecency:set".into(),
                    name: format!("Set weight to {:.1}", clamped),
                    description: Some(format!("Current: {:.1}", config.providers.frecency_weight)),
                    icon: Some(PROVIDER_ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:providers.frecency_weight:{}", clamped),
                    terminal: false,
                    tags: vec!["config".into()],
                });
            }
        } else {
            items.push(LaunchItem {
                id: "config:frecency:weight".into(),
                name: format!("Weight: {:.1}", config.providers.frecency_weight),
                description: Some("Type a value (0.0–1.0) after :config frecency".into()),
                icon: Some(PROVIDER_ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: String::new(),
                terminal: false,
                tags: vec!["config".into()],
            });
        }

        items
    }

    fn numeric_item(&self, label: &str, key: &str, current: i32, input: &str) -> Vec<LaunchItem> {
        if !input.is_empty() {
            if let Ok(v) = input.parse::<i32>() {
                return vec![LaunchItem {
                    id: format!("config:set:{}", key),
                    name: format!("Set {} to {}", label, v),
                    description: Some(format!("Current: {} (restart to apply)", current)),
                    icon: Some(PROVIDER_ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: format!("CONFIG:set:{}:{}", key, v),
                    terminal: false,
                    tags: vec!["config".into()],
                }];
            }
        }

        vec![LaunchItem {
            id: format!("config:show:{}", key),
            name: format!("{}: {}", label, current),
            description: Some(format!("Type a number after :config {} to change (restart to apply)", key.rsplit('.').next().unwrap_or(key))),
            icon: Some(PROVIDER_ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: String::new(),
            terminal: false,
            tags: vec!["config".into()],
        }]
    }
```

- [ ] **Step 7: Implement profile_items and profile_detail_items**

```rust
    fn profile_items(&self, config: &Config, filter: &str) -> Vec<LaunchItem> {
        let mut items: Vec<LaunchItem> = config.profiles.iter()
            .filter(|(name, _)| filter.is_empty() || name.contains(filter))
            .map(|(name, profile)| {
                let modes = profile.modes.join(", ");
                LaunchItem {
                    id: format!("config:profile:{}", name),
                    name: name.clone(),
                    description: Some(if modes.is_empty() { "(no modes)".into() } else { modes }),
                    icon: Some(PROVIDER_ICON.into()),
                    provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                    command: String::new(), // navigate deeper by typing :config profile <name>
                    terminal: false,
                    tags: vec!["config".into(), "profile".into()],
                }
            })
            .collect();

        // "Create" action — user types :config profile create <name>
        items.push(LaunchItem {
            id: "config:profile:create_hint".into(),
            name: "➕ Create New Profile".into(),
            description: Some("Type: :config profile create <name>".into()),
            icon: Some(PROVIDER_ICON.into()),
            provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
            command: String::new(),
            terminal: false,
            tags: vec!["config".into()],
        });

        items
    }

    fn profile_detail_items(&self, config: &Config, rest: &str) -> Vec<LaunchItem> {
        let (profile_name, sub) = match rest.split_once(' ') {
            Some((n, s)) => (n, s.trim()),
            None => (rest, ""),
        };

        // Handle "profile create <name>"
        if profile_name == "create" && !sub.is_empty() {
            return vec![LaunchItem {
                id: format!("config:profile:create:{}", sub),
                name: format!("Create profile '{}'", sub),
                description: Some("Press Enter to create".into()),
                icon: Some(PROVIDER_ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: format!("CONFIG:profile:create:{}", sub),
                terminal: false,
                tags: vec!["config".into()],
            }];
        }

        let profile = match config.profiles.get(profile_name) {
            Some(p) => p,
            None => return vec![],
        };

        if sub == "modes" || sub.starts_with("modes") {
            // Mode checklist
            let all_modes = [
                "app", "cmd", "calc", "conv", "sys", "web", "ssh", "clip",
                "bm", "emoji", "scripts", "file", "uuctl", "media", "weather", "pomo",
            ];
            return all_modes.iter()
                .map(|mode| {
                    let enabled = profile.modes.iter().any(|m| m == mode);
                    let prefix = if enabled { "✓" } else { "✗" };
                    LaunchItem {
                        id: format!("config:profile:{}:mode:{}", profile_name, mode),
                        name: format!("{} {}", prefix, mode),
                        description: Some(format!("{} in profile '{}'", if enabled { "Enabled" } else { "Disabled" }, profile_name)),
                        icon: Some(PROVIDER_ICON.into()),
                        provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                        command: format!("CONFIG:profile:mode:{}:toggle:{}", profile_name, mode),
                        terminal: false,
                        tags: vec!["config".into()],
                    }
                })
                .collect();
        }

        // Profile actions
        vec![
            LaunchItem {
                id: format!("config:profile:{}:modes", profile_name),
                name: "Edit Modes".into(),
                description: Some(format!("Current: {}", profile.modes.join(", "))),
                icon: Some(PROVIDER_ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: String::new(), // navigate with :config profile <name> modes
                terminal: false,
                tags: vec!["config".into()],
            },
            LaunchItem {
                id: format!("config:profile:{}:delete", profile_name),
                name: format!("Delete profile '{}'", profile_name),
                description: Some("Remove this profile".into()),
                icon: Some(PROVIDER_ICON.into()),
                provider: ProviderType::Plugin(PROVIDER_TYPE_ID.into()),
                command: format!("CONFIG:profile:delete:{}", profile_name),
                terminal: false,
                tags: vec!["config".into()],
            },
        ]
    }
```

- [ ] **Step 8: Write tests**

Add at the end of `config_editor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> Arc<RwLock<Config>> {
        Arc::new(RwLock::new(Config::default()))
    }

    #[test]
    fn test_top_level_categories() {
        let p = ConfigProvider::new(make_config());
        let items = p.query("");
        assert!(items.len() >= 8);
        assert!(items.iter().any(|i| i.name == "Providers"));
        assert!(items.iter().any(|i| i.name == "Theme"));
        assert!(items.iter().any(|i| i.name == "Profiles"));
    }

    #[test]
    fn test_provider_toggles() {
        let p = ConfigProvider::new(make_config());
        let items = p.query("providers");
        assert!(items.len() >= 10);
        assert!(items.iter().any(|i| i.name.contains("Calculator")));
    }

    #[test]
    fn test_toggle_action() {
        let config = make_config();
        let p = ConfigProvider::new(Arc::clone(&config));
        assert!(config.read().unwrap().providers.calculator);
        assert!(p.execute_action("CONFIG:toggle:providers.calculator"));
        assert!(!config.read().unwrap().providers.calculator);
        assert!(p.execute_action("CONFIG:toggle:providers.calculator"));
        assert!(config.read().unwrap().providers.calculator);
    }

    #[test]
    fn test_set_theme() {
        let config = make_config();
        let p = ConfigProvider::new(Arc::clone(&config));
        assert!(p.execute_action("CONFIG:set:appearance.theme:nord"));
        assert_eq!(config.read().unwrap().appearance.theme, Some("nord".into()));
    }

    #[test]
    fn test_set_numeric() {
        let config = make_config();
        let p = ConfigProvider::new(Arc::clone(&config));
        assert!(p.execute_action("CONFIG:set:appearance.font_size:18"));
        assert_eq!(config.read().unwrap().appearance.font_size, 18);
    }

    #[test]
    fn test_frecency_weight_clamped() {
        let config = make_config();
        let p = ConfigProvider::new(Arc::clone(&config));
        assert!(p.execute_action("CONFIG:set:providers.frecency_weight:2.0"));
        assert_eq!(config.read().unwrap().providers.frecency_weight, 1.0);
    }

    #[test]
    fn test_invalid_action() {
        let p = ConfigProvider::new(make_config());
        assert!(!p.execute_action("INVALID:something"));
        assert!(!p.execute_action("CONFIG:toggle:nonexistent.key"));
    }

    #[test]
    fn test_theme_items_show_current() {
        let config = make_config();
        {
            config.write().unwrap().appearance.theme = Some("nord".into());
        }
        let p = ConfigProvider::new(config);
        let items = p.query("theme");
        let nord = items.iter().find(|i| i.name.contains("nord")).unwrap();
        assert!(nord.name.starts_with("● "));
    }

    #[test]
    fn test_numeric_input_generates_set_action() {
        let p = ConfigProvider::new(make_config());
        let items = p.query("fontsize 18");
        assert_eq!(items.len(), 1);
        assert!(items[0].name.contains("Set Font Size to 18"));
        assert_eq!(items[0].command, "CONFIG:set:appearance.font_size:18");
    }

    #[test]
    fn test_profile_create() {
        let config = make_config();
        let p = ConfigProvider::new(Arc::clone(&config));
        assert!(p.execute_action("CONFIG:profile:create:myprofile"));
        assert!(config.read().unwrap().profiles.contains_key("myprofile"));
    }

    #[test]
    fn test_profile_delete() {
        let config = make_config();
        {
            config.write().unwrap().profiles.insert("test".into(), crate::config::ProfileConfig { modes: vec!["app".into()] });
        }
        let p = ConfigProvider::new(Arc::clone(&config));
        assert!(p.execute_action("CONFIG:profile:delete:test"));
        assert!(!config.read().unwrap().profiles.contains_key("test"));
    }

    #[test]
    fn test_profile_mode_toggle() {
        let config = make_config();
        {
            config.write().unwrap().profiles.insert("dev".into(), crate::config::ProfileConfig { modes: vec!["app".into()] });
        }
        let p = ConfigProvider::new(Arc::clone(&config));
        // Add ssh
        assert!(p.execute_action("CONFIG:profile:mode:dev:toggle:ssh"));
        assert!(config.read().unwrap().profiles["dev"].modes.contains(&"ssh".into()));
        // Remove app
        assert!(p.execute_action("CONFIG:profile:mode:dev:toggle:app"));
        assert!(!config.read().unwrap().profiles["dev"].modes.contains(&"app".into()));
    }

    #[test]
    fn test_provider_type() {
        let p = ConfigProvider::new(make_config());
        assert_eq!(p.provider_type(), ProviderType::Plugin("config".into()));
    }

    #[test]
    fn test_profile_create_query() {
        let p = ConfigProvider::new(make_config());
        let items = p.query("profile create myname");
        assert_eq!(items.len(), 1);
        assert!(items[0].name.contains("myname"));
        assert_eq!(items[0].command, "CONFIG:profile:create:myname");
    }
}
```

- [ ] **Step 9: Verify compilation and tests**

Run: `cargo test -p owlry-core config_editor`

Note: tests that call `execute_action` will try `config.save()` which writes to disk. The save will fail gracefully (warns) in test environment since there's no XDG config dir — the toggle/set still returns true. If tests fail due to save, add `#[allow(dead_code)]` or mock the save path. Alternatively, since `Config::save()` returns a Result and the provider logs but ignores errors, this should be fine.

Expected: All tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/owlry-core/src/providers/config_editor.rs crates/owlry-core/src/providers/mod.rs
git commit -m "feat(core): add built-in config editor provider

Interactive :config prefix for browsing and modifying settings.
Supports provider toggles, theme/engine selection, numeric input,
and profile CRUD. Uses CONFIG:* action commands that persist to
config.toml via Config::save()."
```

---

### Task 2: Wire ConfigProvider into ProviderManager

**Files:**
- Modify: `crates/owlry-core/src/providers/mod.rs`
- Modify: `crates/owlry-core/src/config/mod.rs` (if ProfileConfig is not public)

The ConfigProvider needs to be:
1. Registered as a built-in dynamic provider
2. Its `execute_action` called from `execute_plugin_action`

- [ ] **Step 1: Make Config wrap in Arc<RwLock> for shared ownership**

The ConfigProvider needs mutable access to config. Currently `new_with_config` takes `&Config`. Change the daemon startup to wrap Config in `Arc<RwLock<Config>>` and pass it to both the ConfigProvider and the server.

In `crates/owlry-core/src/providers/mod.rs`, in `new_with_config()`, after creating the config provider:

```rust
        // Config editor — needs shared mutable access to config
        let config_arc = std::sync::Arc::new(std::sync::RwLock::new(config.clone()));
        builtin_dynamic.push(Box::new(config_editor::ConfigProvider::new(config_arc)));
        info!("Registered built-in config editor provider");
```

- [ ] **Step 2: Extend execute_plugin_action for built-in providers**

In `execute_plugin_action`, after the existing native provider check, add:

```rust
        // Check built-in config editor
        if command.starts_with("CONFIG:") {
            for provider in &self.builtin_dynamic {
                if let ProviderType::Plugin(ref id) = provider.provider_type() {
                    if id == "config" {
                        // Downcast to ConfigProvider to call execute_action
                        // Since we can't downcast trait objects easily, add an
                        // execute_action method to DynamicProvider with default impl
                        return provider.execute_action(command);
                    }
                }
            }
        }
```

For this to work, add `execute_action` to the `DynamicProvider` trait with a default no-op:

```rust
pub(crate) trait DynamicProvider: Send + Sync {
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn query(&self, query: &str) -> Vec<LaunchItem>;
    fn priority(&self) -> u32;

    /// Handle a plugin action command. Returns true if handled.
    fn execute_action(&self, _command: &str) -> bool {
        false
    }
}
```

The ConfigProvider already has `execute_action` as an inherent method — just also implement it via the trait.

- [ ] **Step 3: Ensure ProfileConfig is accessible**

Check if `crate::config::ProfileConfig` is public. If not, add `pub` to its definition in `config/mod.rs`. The ConfigProvider needs to construct it for profile creation.

- [ ] **Step 4: Run tests**

Run: `cargo test -p owlry-core --lib`

Expected: All tests pass (128+ existing + new config editor tests).

- [ ] **Step 5: Commit**

```bash
git add crates/owlry-core/src/providers/mod.rs crates/owlry-core/src/config/mod.rs
git commit -m "feat(core): wire config editor into ProviderManager

Register ConfigProvider as built-in dynamic provider. Extend
execute_plugin_action to dispatch CONFIG:* commands. Add
execute_action method to DynamicProvider trait."
```

---

### Task 3: Update CLAUDE.md and README with config editor docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add config editor section to README**

In the README, in the Usage section (after Keyboard Shortcuts), add:

```markdown
### Settings Editor

Type `:config` to browse and modify settings without editing files:

| Command | What it does |
|---------|-------------|
| `:config` | Show all setting categories |
| `:config providers` | Toggle providers on/off |
| `:config theme` | Select color theme |
| `:config engine` | Select web search engine |
| `:config frecency` | Toggle frecency, set weight |
| `:config fontsize 16` | Set font size (restart to apply) |
| `:config profiles` | List profiles |
| `:config profile create dev` | Create a new profile |
| `:config profile dev modes` | Edit which modes a profile includes |

Changes are saved to `config.toml` immediately. Some settings (theme, frecency) take effect on the next search. Others (font size, dimensions) require a restart.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add config editor usage to README"
```

---

## Execution Notes

### Task dependency order

Task 1 is the bulk of the implementation. Task 2 wires it in. Task 3 is docs.

**Order:** 1 → 2 → 3

### What's NOT in this plan

- **Hot-apply for theme** — would need the UI to re-trigger CSS loading after a CONFIG action. Can be added later by emitting a signal from the daemon or having the UI check a flag after `execute_plugin_action` returns.
- **Profile rename via text input** — the current design supports `:config profile create <name>` but rename would need a two-step flow. Can be added later.
- **Config file watching** — if the user edits `config.toml` externally, the ConfigProvider's cached `Arc<RwLock<Config>>` becomes stale. A file watcher could reload it. Deferred.
