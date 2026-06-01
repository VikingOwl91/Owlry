//! Data layer for the Lua config: the [`LuaConfig`] struct that accumulates
//! state from `owlry.set` / `owlry.providers` / `owlry.tabs` / `owlry.theme`
//! / `owlry.profiles` / `owlry.provider` calls, plus the merge that overlays
//! it onto the existing TOML-derived [`crate::config::Config`].
//!
//! Mapping mirrors `docs/lua-api.md` §4.1–§4.5 and §6.5. Every settable
//! key is an `Option<T>`: `None` means "the user didn't touch it, keep the
//! base value."

use std::collections::HashMap;

use crate::config::{Config, ProfileConfig, ThemeColors};

/// Accumulated state from `owlry.lua`. Built up by the closures registered in
/// [`super::api::install`] and drained via [`super::runtime::LuaContext::snapshot`].
#[derive(Debug, Default, Clone)]
pub struct LuaConfig {
    // ─── owlry.set { ... } ─────────────────────────────────────────────────
    pub theme: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub font_size: Option<u32>,
    pub border_radius: Option<u32>,
    pub terminal: Option<String>,
    pub use_uwsm: Option<bool>,
    pub show_icons: Option<bool>,
    pub max_results: Option<usize>,
    pub frecency: Option<bool>,
    pub frecency_weight: Option<f64>,
    pub search_engine: Option<String>,
    pub filesearch_roots: Option<Vec<String>>,

    /// Keys passed to `owlry.set` that we don't recognise. Surfaced by
    /// `owlry config validate` (Phase 3.9). Not an error — forward-compat.
    pub unknown_settings: Vec<String>,

    /// `None` = `owlry.providers` was never called → keep base defaults.
    /// `Some(list)` = explicit set; ids absent from the list are disabled.
    pub providers: Option<Vec<String>>,

    /// `None` = `owlry.tabs` was never called → keep base defaults.
    /// `Some(list)` = explicit set (may be empty to hide all tabs).
    pub tabs: Option<Vec<String>>,

    /// User providers registered via `owlry.provider { ... }`. Order matches
    /// registration. Duplicates by `id` are resolved at registration time:
    /// later wins, earlier is dropped (warning logged).
    pub user_providers: Vec<LuaProviderSpec>,

    /// IDs that appeared more than once in `owlry.provider { ... }` calls.
    /// Stored separately so `owlry config validate` (Phase 3.9) can surface
    /// them — the dedup in [`super::api::apply_provider`] would otherwise
    /// erase the evidence by the time the snapshot is read.
    pub duplicate_user_provider_ids: Vec<String>,

    // ─── owlry.theme(...) ─────────────────────────────────────────────────
    /// Theme name from the string form `owlry.theme("nord")`. Last write wins
    /// across multiple calls (string forms merge with table forms — see
    /// §4.5: "call owlry.theme(name) first, then owlry.theme { ... } to
    /// layer overrides on top").
    pub theme_name: Option<String>,

    /// Inline colour overrides from the table form `owlry.theme { ... }`.
    /// Per-key last-write-wins. Empty by default (no overrides).
    pub theme_colors: ThemeColors,

    /// Theme keys we don't recognise (forward-compat for 2.2+ palette
    /// additions). Surfaced by `owlry config validate` (Phase 3.9).
    pub unknown_theme_keys: Vec<String>,

    // ─── owlry.profiles { ... } ───────────────────────────────────────────
    /// Named profiles from `owlry.profiles { dev = { ... }, ... }`. Each
    /// profile maps to a list of provider ids used when `--profile <name>`
    /// is passed. `None` = profiles were never set in Lua → keep TOML or
    /// defaults. `Some(map)` replaces the base profiles (consistent with
    /// `providers`/`tabs` replace-on-call semantics).
    pub profiles: Option<HashMap<String, Vec<String>>>,
}

/// Captured spec from a single `owlry.provider { ... }` call.
///
/// Holds the `mlua::Function` for the `items` callback alive — that keeps
/// the parent Lua state alive too (Function holds an internal Arc).
#[derive(Clone)]
pub struct LuaProviderSpec {
    pub id: String,
    pub name: Option<String>,
    pub prefix: Option<String>,
    pub tab_label: Option<String>,
    pub icon: Option<String>,
    pub search_noun: Option<String>,
    pub priority: u32,
    /// 2.1 supports only `false` (static, cached after first refresh).
    /// `true` errors at registration time (per docs/lua-api.md §4.4).
    pub dynamic: bool,
    pub items_fn: mlua::Function,
}

impl std::fmt::Debug for LuaProviderSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaProviderSpec")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("prefix", &self.prefix)
            .field("tab_label", &self.tab_label)
            .field("icon", &self.icon)
            .field("search_noun", &self.search_noun)
            .field("priority", &self.priority)
            .field("dynamic", &self.dynamic)
            .field("items_fn", &"<lua function>")
            .finish()
    }
}

/// Validate a user-supplied provider id. Spec §4.4: "Must be lowercase,
/// alphanumeric + `-`/`_`."
pub(crate) fn is_valid_provider_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Known keys accepted by `owlry.set`. Anything else lands in
/// [`LuaConfig::unknown_settings`].
pub(crate) const KNOWN_SET_KEYS: &[&str] = &[
    "theme",
    "width",
    "height",
    "font_size",
    "border_radius",
    "terminal",
    "use_uwsm",
    "show_icons",
    "max_results",
    "frecency",
    "frecency_weight",
    "search_engine",
    "filesearch_roots",
];

/// Known keys accepted by `owlry.theme { ... }`. Mirrors the fields of
/// [`ThemeColors`]; the pre-v2 `badge_sys` alias is included so existing
/// configs translate cleanly (it maps to `badge_power` per the v2 rename).
pub(crate) const KNOWN_THEME_KEYS: &[&str] = &[
    "background",
    "background_secondary",
    "border",
    "text",
    "text_secondary",
    "accent",
    "accent_bright",
    "badge_app",
    "badge_bookmark",
    "badge_calc",
    "badge_clip",
    "badge_cmd",
    "badge_dmenu",
    "badge_emoji",
    "badge_file",
    "badge_script",
    "badge_ssh",
    "badge_power",
    "badge_sys", // pre-v2 alias, normalised to badge_power at merge time
    "badge_uuctl",
    "badge_web",
    "badge_media",
    "badge_weather",
    "badge_pomo",
];

impl LuaConfig {
    /// Apply the Lua-side overrides on top of an existing [`Config`].
    ///
    /// Fields the user didn't set (`None`) are left untouched. `providers`
    /// and `tabs` lists, when present, fully replace the corresponding base
    /// state (per §4.2 / §4.3 — "calling replaces, it's not additive").
    pub fn merge_into(&self, cfg: &mut Config) {
        // ── owlry.set ──────────────────────────────────────────────────
        if let Some(v) = &self.theme {
            cfg.appearance.theme = Some(v.clone());
        }
        if let Some(v) = self.width {
            cfg.appearance.width = v;
        }
        if let Some(v) = self.height {
            cfg.appearance.height = v;
        }
        if let Some(v) = self.font_size {
            cfg.appearance.font_size = v;
        }
        if let Some(v) = self.border_radius {
            cfg.appearance.border_radius = v;
        }
        if let Some(v) = &self.terminal {
            cfg.general.terminal_command = Some(v.clone());
        }
        if let Some(v) = self.use_uwsm {
            cfg.general.use_uwsm = v;
        }
        if let Some(v) = self.show_icons {
            cfg.general.show_icons = v;
        }
        if let Some(v) = self.max_results {
            cfg.general.max_results = v;
        }
        if let Some(v) = self.frecency {
            cfg.providers.frecency = v;
        }
        if let Some(v) = self.frecency_weight {
            cfg.providers.frecency_weight = v;
        }
        if let Some(v) = &self.search_engine {
            cfg.providers.search_engine = v.clone();
        }
        if let Some(v) = &self.filesearch_roots {
            cfg.providers.filesearch_roots = v.clone();
        }

        // ── owlry.providers ────────────────────────────────────────────
        if let Some(list) = &self.providers {
            apply_providers_list(&mut cfg.providers, list);
        }

        // ── owlry.tabs ─────────────────────────────────────────────────
        if let Some(list) = &self.tabs {
            cfg.general.tabs = list.clone();
        }

        // ── owlry.theme(...) ───────────────────────────────────────────
        if let Some(name) = &self.theme_name {
            cfg.appearance.theme = Some(name.clone());
        }
        merge_theme_colors(&self.theme_colors, &mut cfg.appearance.colors);

        // ── owlry.profiles { ... } ─────────────────────────────────────
        // Replace base profiles entirely (consistent with providers/tabs
        // replace-on-call). Profile values become `ProfileConfig { modes }`.
        if let Some(profiles) = &self.profiles {
            cfg.profiles.clear();
            for (name, modes) in profiles {
                cfg.profiles.insert(
                    name.clone(),
                    ProfileConfig {
                        modes: modes.clone(),
                    },
                );
            }
        }
    }
}

/// Overlay any `Some` colour fields from the Lua side onto the base
/// [`ThemeColors`]. `None` fields are skipped so the base theme survives.
fn merge_theme_colors(src: &ThemeColors, dst: &mut ThemeColors) {
    macro_rules! merge {
        ($field:ident) => {
            if let Some(ref v) = src.$field {
                dst.$field = Some(v.clone());
            }
        };
    }
    merge!(background);
    merge!(background_secondary);
    merge!(border);
    merge!(text);
    merge!(text_secondary);
    merge!(accent);
    merge!(accent_bright);
    merge!(badge_app);
    merge!(badge_bookmark);
    merge!(badge_calc);
    merge!(badge_clip);
    merge!(badge_cmd);
    merge!(badge_dmenu);
    merge!(badge_emoji);
    merge!(badge_file);
    merge!(badge_script);
    merge!(badge_ssh);
    merge!(badge_power);
    merge!(badge_uuctl);
    merge!(badge_web);
    merge!(badge_media);
    merge!(badge_weather);
    merge!(badge_pomo);
}

/// Translate a list of provider IDs (as the user wrote them in
/// `owlry.providers { ... }`) into the per-provider boolean flags on
/// [`crate::config::ProvidersConfig`]. Ids not present in the list are
/// disabled. Honours v1→v2 aliases (`sys`/`system` → `power`,
/// `uuctl` → `systemd`).
fn apply_providers_list(p: &mut crate::config::ProvidersConfig, list: &[String]) {
    // Start from "everything off"; the list determines who flips on.
    p.applications = false;
    p.commands = false;
    p.calculator = false;
    p.converter = false;
    p.power = false;
    p.systemd = false;
    p.clipboard = false;
    p.emoji = false;
    p.filesearch = false;
    p.ssh = false;
    p.websearch = false;

    for id in list {
        match id.as_str() {
            "app" | "application" | "applications" => p.applications = true,
            "cmd" | "command" | "commands" => p.commands = true,
            "calc" | "calculator" => p.calculator = true,
            "conv" | "converter" => p.converter = true,
            "power" | "sys" | "system" => p.power = true,
            "systemd" | "uuctl" => p.systemd = true,
            "ssh" => p.ssh = true,
            "clipboard" | "clip" => p.clipboard = true,
            "emoji" => p.emoji = true,
            "filesearch" | "file" => p.filesearch = true,
            "websearch" | "web" | "search" => p.websearch = true,
            // dmenu is a CLI-mode-only provider with no ProvidersConfig bool.
            // Unknown ids are tolerated here; validation (Phase 3.9) catches them.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lua_config_is_empty() {
        let lc = LuaConfig::default();
        assert!(lc.theme.is_none());
        assert!(lc.width.is_none());
        assert!(lc.providers.is_none());
        assert!(lc.tabs.is_none());
        assert!(lc.unknown_settings.is_empty());
    }

    #[test]
    fn empty_lua_config_leaves_base_config_untouched() {
        let mut cfg = Config::default();
        let base_width = cfg.appearance.width;
        let base_max = cfg.general.max_results;
        let base_app = cfg.providers.applications;

        LuaConfig::default().merge_into(&mut cfg);

        assert_eq!(cfg.appearance.width, base_width);
        assert_eq!(cfg.general.max_results, base_max);
        assert_eq!(cfg.providers.applications, base_app);
    }

    #[test]
    fn merge_overrides_appearance_settings() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            theme: Some("catppuccin-mocha".into()),
            width: Some(900),
            height: Some(700),
            font_size: Some(16),
            border_radius: Some(8),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert_eq!(cfg.appearance.theme.as_deref(), Some("catppuccin-mocha"));
        assert_eq!(cfg.appearance.width, 900);
        assert_eq!(cfg.appearance.height, 700);
        assert_eq!(cfg.appearance.font_size, 16);
        assert_eq!(cfg.appearance.border_radius, 8);
    }

    #[test]
    fn merge_overrides_general_settings() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            terminal: Some("kitty".into()),
            use_uwsm: Some(true),
            show_icons: Some(false),
            max_results: Some(50),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert_eq!(cfg.general.terminal_command.as_deref(), Some("kitty"));
        assert!(cfg.general.use_uwsm);
        assert!(!cfg.general.show_icons);
        assert_eq!(cfg.general.max_results, 50);
    }

    #[test]
    fn merge_overrides_frecency_and_search() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            frecency: Some(false),
            frecency_weight: Some(0.75),
            search_engine: Some("google".into()),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert!(!cfg.providers.frecency);
        assert!((cfg.providers.frecency_weight - 0.75).abs() < f64::EPSILON);
        assert_eq!(cfg.providers.search_engine, "google");
    }

    #[test]
    fn merge_overrides_filesearch_roots() {
        let mut cfg = Config::default();
        assert!(cfg.providers.filesearch_roots.is_empty());
        let lc = LuaConfig {
            filesearch_roots: Some(vec!["~/Documents".into(), "~/code".into()]),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);
        assert_eq!(
            cfg.providers.filesearch_roots,
            vec!["~/Documents".to_string(), "~/code".to_string()]
        );
    }

    #[test]
    fn providers_list_enables_only_listed_ids() {
        let mut cfg = Config::default();
        // Defaults: everything is true. Lua list specifies only a subset.
        let lc = LuaConfig {
            providers: Some(vec!["app".into(), "cmd".into(), "calc".into()]),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert!(cfg.providers.applications);
        assert!(cfg.providers.commands);
        assert!(cfg.providers.calculator);
        // Not listed → disabled.
        assert!(!cfg.providers.converter);
        assert!(!cfg.providers.power);
        assert!(!cfg.providers.systemd);
        assert!(!cfg.providers.ssh);
        assert!(!cfg.providers.emoji);
        assert!(!cfg.providers.clipboard);
        assert!(!cfg.providers.filesearch);
        assert!(!cfg.providers.websearch);
    }

    #[test]
    fn providers_list_honours_v1_aliases() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            // sys/system → power; uuctl → systemd
            providers: Some(vec!["sys".into(), "uuctl".into()]),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert!(cfg.providers.power);
        assert!(cfg.providers.systemd);
        // None of the actually-listed canonical ids show up:
        assert!(!cfg.providers.applications);
        assert!(!cfg.providers.commands);
    }

    #[test]
    fn providers_list_ignores_unknown_ids() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            providers: Some(vec!["app".into(), "fictional_provider".into()]),
            ..Default::default()
        };
        // Should not panic; just silently skips the unknown.
        lc.merge_into(&mut cfg);

        assert!(cfg.providers.applications);
    }

    #[test]
    fn empty_providers_list_disables_everything() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            providers: Some(vec![]),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert!(!cfg.providers.applications);
        assert!(!cfg.providers.commands);
        assert!(!cfg.providers.power);
        assert!(!cfg.providers.systemd);
    }

    #[test]
    fn tabs_list_replaces_tab_order() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            tabs: Some(vec!["app".into(), "systemd".into(), "ssh".into()]),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert_eq!(cfg.general.tabs, vec!["app", "systemd", "ssh"]);
    }

    #[test]
    fn empty_tabs_list_hides_all_tabs() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            tabs: Some(vec![]),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert!(cfg.general.tabs.is_empty());
    }

    #[test]
    fn valid_provider_id_accepts_lowercase_alnum_dash_underscore() {
        assert!(is_valid_provider_id("hs"));
        assert!(is_valid_provider_id("my-provider"));
        assert!(is_valid_provider_id("my_provider"));
        assert!(is_valid_provider_id("p1"));
        assert!(is_valid_provider_id("a-b_c1"));
    }

    #[test]
    fn valid_provider_id_rejects_invalid_shapes() {
        assert!(!is_valid_provider_id(""));
        assert!(!is_valid_provider_id("Capital"));
        assert!(!is_valid_provider_id("has space"));
        assert!(!is_valid_provider_id("dot.in.id"));
        assert!(!is_valid_provider_id("colon:bad"));
        assert!(!is_valid_provider_id("emoji_💀"));
    }

    // ── owlry.theme(...) merge semantics ────────────────────────────────────

    #[test]
    fn theme_name_overrides_base_appearance_theme() {
        let mut cfg = Config::default();
        let lc = LuaConfig {
            theme_name: Some("catppuccin-mocha".into()),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);
        assert_eq!(cfg.appearance.theme.as_deref(), Some("catppuccin-mocha"));
    }

    #[test]
    fn theme_colours_overlay_only_set_fields() {
        let mut cfg = Config::default();
        // Base theme has colours already set:
        cfg.appearance.colors.background = Some("#000000".into());
        cfg.appearance.colors.text = Some("#ffffff".into());

        // Lua overlays only the accent — background and text must survive.
        let mut lua_colors = ThemeColors::default();
        lua_colors.accent = Some("#ff00ff".into());
        let lc = LuaConfig {
            theme_colors: lua_colors,
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert_eq!(cfg.appearance.colors.background.as_deref(), Some("#000000"));
        assert_eq!(cfg.appearance.colors.text.as_deref(), Some("#ffffff"));
        assert_eq!(cfg.appearance.colors.accent.as_deref(), Some("#ff00ff"));
    }

    #[test]
    fn theme_name_and_colours_compose() {
        let mut cfg = Config::default();
        let mut lua_colors = ThemeColors::default();
        lua_colors.background = Some("#1e1e2e".into());
        let lc = LuaConfig {
            theme_name: Some("nord".into()),
            theme_colors: lua_colors,
            ..Default::default()
        };
        lc.merge_into(&mut cfg);
        assert_eq!(cfg.appearance.theme.as_deref(), Some("nord"));
        assert_eq!(cfg.appearance.colors.background.as_deref(), Some("#1e1e2e"));
    }

    // ── owlry.profiles { ... } merge semantics ──────────────────────────────

    #[test]
    fn profiles_map_replaces_base_profiles() {
        let mut cfg = Config::default();
        // Base TOML had a "legacy" profile that must be cleared.
        cfg.profiles.insert(
            "legacy".into(),
            ProfileConfig {
                modes: vec!["app".into()],
            },
        );

        let mut profiles: HashMap<String, Vec<String>> = HashMap::new();
        profiles.insert("dev".into(), vec!["app".into(), "cmd".into(), "ssh".into()]);
        profiles.insert("media".into(), vec!["emoji".into(), "clipboard".into()]);
        let lc = LuaConfig {
            profiles: Some(profiles),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);

        assert!(
            !cfg.profiles.contains_key("legacy"),
            "Lua profiles must replace base entries entirely"
        );
        assert_eq!(cfg.profiles.len(), 2);
        assert_eq!(
            cfg.profiles.get("dev").map(|p| p.modes.as_slice()),
            Some(&["app".to_string(), "cmd".to_string(), "ssh".to_string()][..])
        );
        assert_eq!(
            cfg.profiles.get("media").map(|p| p.modes.as_slice()),
            Some(&["emoji".to_string(), "clipboard".to_string()][..])
        );
    }

    #[test]
    fn empty_profiles_clears_base_profiles() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "legacy".into(),
            ProfileConfig {
                modes: vec!["app".into()],
            },
        );
        let lc = LuaConfig {
            profiles: Some(HashMap::new()),
            ..Default::default()
        };
        lc.merge_into(&mut cfg);
        assert!(cfg.profiles.is_empty());
    }

    #[test]
    fn omitted_profiles_keeps_base_profiles() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "legacy".into(),
            ProfileConfig {
                modes: vec!["app".into()],
            },
        );
        let lc = LuaConfig::default(); // profiles: None
        lc.merge_into(&mut cfg);
        assert!(cfg.profiles.contains_key("legacy"));
    }
}
