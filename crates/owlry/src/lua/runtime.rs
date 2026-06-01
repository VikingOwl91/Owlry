//! Lua runtime wrapper.
//!
//! [`LuaContext`] owns a single `mlua::Lua` state with the `owlry.*` API
//! installed and an `Arc<Mutex<LuaConfig>>` shared with the registered
//! closures. Evaluate user files via [`LuaContext::eval_file`] (or
//! [`LuaContext::eval_str`] for tests) and snapshot the accumulated state
//! via [`LuaContext::snapshot`].
//!
//! The Lua state is wrapped in [`Arc`] so [`super::provider::LuaProvider`]
//! instances can keep it alive independently of the context. `mlua::Function`
//! references don't bump the Lua refcount on their own — drop the state and
//! they panic at call time. Holding an `Arc<Lua>` in each provider closes
//! that gap.

use std::path::Path;
use std::sync::{Arc, Mutex};

use mlua::Lua;

use super::api;
use super::config::LuaConfig;
use super::error::LuaConfigError;

pub struct LuaContext {
    lua: Arc<Lua>,
    state: Arc<Mutex<LuaConfig>>,
}

impl LuaContext {
    /// Build a fresh Lua state with the `owlry.*` API surface installed.
    pub fn new() -> Result<Self, LuaConfigError> {
        let lua = Arc::new(Lua::new());
        let state = Arc::new(Mutex::new(LuaConfig::default()));
        api::install(&lua, Arc::clone(&state))?;
        Ok(Self { lua, state })
    }

    /// Clone the inner Lua handle. Used by [`super::provider::LuaProvider`]
    /// to keep the state alive for the lifetime of the provider.
    pub fn lua_handle(&self) -> Arc<Lua> {
        Arc::clone(&self.lua)
    }

    /// Evaluate a Lua file against this context. Reads from disk, runs the
    /// script with the file path as its chunk name so errors point at the
    /// right place.
    pub fn eval_file(&self, path: &Path) -> Result<(), LuaConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| LuaConfigError::Read {
            path: path.to_path_buf(),
            source: e,
        })?;
        let name = path.to_string_lossy().into_owned();
        self.lua
            .load(&content)
            .set_name(name)
            .exec()
            .map_err(|e| LuaConfigError::Eval {
                path: path.to_path_buf(),
                source: e,
            })?;
        Ok(())
    }

    /// Evaluate a Lua source string. Primarily for tests; production code
    /// goes through [`Self::eval_file`].
    #[cfg(test)]
    pub fn eval_str(&self, src: &str) -> Result<(), LuaConfigError> {
        self.lua.load(src).exec()?;
        Ok(())
    }

    /// Take a snapshot of the accumulated config state. The context can be
    /// re-used afterwards (Phase 3.7 hot-reload uses this).
    pub fn snapshot(&self) -> LuaConfig {
        self.state
            .lock()
            .expect("lua config state mutex poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_after(src: &str) -> LuaConfig {
        let ctx = LuaContext::new().expect("lua context must build");
        ctx.eval_str(src).expect("script must run");
        ctx.snapshot()
    }

    #[test]
    fn new_context_starts_empty() {
        let ctx = LuaContext::new().expect("must build");
        let s = ctx.snapshot();
        assert!(s.theme.is_none());
        assert!(s.providers.is_none());
        assert!(s.tabs.is_none());
    }

    #[test]
    fn owlry_is_a_global_table() {
        let ctx = LuaContext::new().expect("must build");
        ctx.eval_str(
            r#"
            assert(type(owlry) == "table", "owlry must be a table")
            assert(type(owlry.set) == "function", "owlry.set must be a function")
            assert(type(owlry.providers) == "function", "owlry.providers must be a function")
            assert(type(owlry.tabs) == "function", "owlry.tabs must be a function")
            "#,
        )
        .expect("assertions must pass");
    }

    #[test]
    fn require_returns_the_owlry_table() {
        let ctx = LuaContext::new().expect("must build");
        ctx.eval_str(
            r#"
            local o = require("owlry")
            assert(o == owlry, "require('owlry') must return the same table as the global")
            "#,
        )
        .expect("require must work");
    }

    #[test]
    fn set_populates_known_keys() {
        let s = snapshot_after(
            r#"
            owlry.set {
              theme = "owl",
              width = 900,
              height = 720,
              font_size = 16,
              border_radius = 8,
              terminal = "kitty",
              use_uwsm = true,
              show_icons = false,
              max_results = 50,
              frecency = false,
              frecency_weight = 0.75,
              search_engine = "google",
            }
            "#,
        );
        assert_eq!(s.theme.as_deref(), Some("owl"));
        assert_eq!(s.width, Some(900));
        assert_eq!(s.height, Some(720));
        assert_eq!(s.font_size, Some(16));
        assert_eq!(s.border_radius, Some(8));
        assert_eq!(s.terminal.as_deref(), Some("kitty"));
        assert_eq!(s.use_uwsm, Some(true));
        assert_eq!(s.show_icons, Some(false));
        assert_eq!(s.max_results, Some(50));
        assert_eq!(s.frecency, Some(false));
        assert_eq!(s.frecency_weight, Some(0.75));
        assert_eq!(s.search_engine.as_deref(), Some("google"));
        assert!(s.unknown_settings.is_empty());
    }

    #[test]
    fn set_merges_across_multiple_calls() {
        let s = snapshot_after(
            r#"
            owlry.set { theme = "owl", width = 900 }
            owlry.set { font_size = 16 }
            owlry.set { width = 1000 }
            "#,
        );
        assert_eq!(
            s.theme.as_deref(),
            Some("owl"),
            "earlier theme must persist"
        );
        assert_eq!(s.font_size, Some(16), "second call must apply");
        assert_eq!(s.width, Some(1000), "later width must win");
    }

    #[test]
    fn set_records_unknown_keys_without_failing() {
        let s = snapshot_after(
            r#"
            owlry.set { theme = "owl", mystery_2_2_key = 42, another_unknown = "x" }
            "#,
        );
        assert_eq!(s.theme.as_deref(), Some("owl"));
        assert!(s.unknown_settings.contains(&"mystery_2_2_key".to_string()));
        assert!(s.unknown_settings.contains(&"another_unknown".to_string()));
    }

    #[test]
    fn set_with_wrong_type_returns_error() {
        let ctx = LuaContext::new().expect("must build");
        let err = ctx
            .eval_str(r#"owlry.set { width = "not_a_number" }"#)
            .unwrap_err();
        assert!(
            matches!(err, LuaConfigError::Runtime(_)),
            "expected Runtime error, got {:?}",
            err
        );
    }

    #[test]
    fn providers_captures_id_list() {
        let s = snapshot_after(r#"owlry.providers { "app", "cmd", "calc", "conv", "power" }"#);
        assert_eq!(
            s.providers,
            Some(vec![
                "app".into(),
                "cmd".into(),
                "calc".into(),
                "conv".into(),
                "power".into(),
            ])
        );
    }

    #[test]
    fn providers_called_twice_replaces_the_list() {
        let s = snapshot_after(
            r#"
            owlry.providers { "app", "cmd" }
            owlry.providers { "ssh", "emoji" }
            "#,
        );
        assert_eq!(
            s.providers,
            Some(vec!["ssh".into(), "emoji".into()]),
            "second call must fully replace first"
        );
    }

    #[test]
    fn tabs_captures_list_preserving_order() {
        let s = snapshot_after(r#"owlry.tabs { "app", "cmd", "uuctl" }"#);
        assert_eq!(
            s.tabs,
            Some(vec!["app".into(), "cmd".into(), "uuctl".into()])
        );
    }

    #[test]
    fn tabs_called_twice_replaces_the_list() {
        let s = snapshot_after(
            r#"
            owlry.tabs { "app", "cmd" }
            owlry.tabs { "ssh" }
            "#,
        );
        assert_eq!(s.tabs, Some(vec!["ssh".into()]));
    }

    #[test]
    fn empty_tabs_list_is_valid() {
        let s = snapshot_after(r#"owlry.tabs { }"#);
        assert_eq!(s.tabs, Some(vec![]));
    }

    #[test]
    fn snapshot_can_be_taken_multiple_times() {
        let ctx = LuaContext::new().expect("must build");
        ctx.eval_str(r#"owlry.set { theme = "owl" }"#).expect("run");
        let s1 = ctx.snapshot();
        let s2 = ctx.snapshot();
        assert_eq!(s1.theme, s2.theme);
        ctx.eval_str(r#"owlry.set { width = 900 }"#).expect("run");
        let s3 = ctx.snapshot();
        assert_eq!(s3.theme.as_deref(), Some("owl"));
        assert_eq!(s3.width, Some(900));
    }

    #[test]
    fn end_to_end_full_config_example() {
        let s = snapshot_after(
            r#"
            local o = require("owlry")
            o.set { theme = "owl", width = 850, font_size = 14 }
            o.providers { "app", "cmd", "calc", "conv", "power", "systemd" }
            o.tabs { "app", "cmd", "uuctl" }
            "#,
        );
        assert_eq!(s.theme.as_deref(), Some("owl"));
        assert_eq!(s.width, Some(850));
        assert_eq!(s.font_size, Some(14));
        assert_eq!(
            s.providers.as_deref(),
            Some(
                &[
                    "app".to_string(),
                    "cmd".to_string(),
                    "calc".to_string(),
                    "conv".to_string(),
                    "power".to_string(),
                    "systemd".to_string(),
                ][..]
            )
        );
        assert_eq!(
            s.tabs.as_deref(),
            Some(&["app".to_string(), "cmd".to_string(), "uuctl".to_string()][..])
        );
    }

    #[test]
    fn eval_file_reads_and_runs_a_real_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("owlry.lua");
        let mut f = std::fs::File::create(&path).expect("create");
        writeln!(f, r#"owlry.set {{ theme = "nord", width = 1024 }}"#).expect("write");
        drop(f);

        let ctx = LuaContext::new().expect("must build");
        ctx.eval_file(&path).expect("file must eval");
        let s = ctx.snapshot();
        assert_eq!(s.theme.as_deref(), Some("nord"));
        assert_eq!(s.width, Some(1024));
    }

    #[test]
    fn eval_file_reports_path_on_syntax_error() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broken.lua");
        let mut f = std::fs::File::create(&path).expect("create");
        writeln!(f, "this is not valid lua {{{{").expect("write");
        drop(f);

        let ctx = LuaContext::new().expect("must build");
        let err = ctx.eval_file(&path).unwrap_err();
        match err {
            LuaConfigError::Eval { path: p, .. } => {
                assert_eq!(p, path);
            }
            other => panic!("expected Eval error, got {:?}", other),
        }
    }

    #[test]
    fn eval_file_reports_path_on_missing_file() {
        let ctx = LuaContext::new().expect("must build");
        let missing = Path::new("/definitely/does/not/exist/owlry.lua");
        let err = ctx.eval_file(missing).unwrap_err();
        match err {
            LuaConfigError::Read { path: p, .. } => {
                assert_eq!(p, missing);
            }
            other => panic!("expected Read error, got {:?}", other),
        }
    }

    // ── owlry.theme(...) end-to-end ─────────────────────────────────────────

    #[test]
    fn theme_string_form_sets_theme_name() {
        let s = snapshot_after(r#"owlry.theme("catppuccin-mocha")"#);
        assert_eq!(s.theme_name.as_deref(), Some("catppuccin-mocha"));
        // Colours and unknowns unaffected.
        assert!(s.theme_colors.background.is_none());
        assert!(s.unknown_theme_keys.is_empty());
    }

    #[test]
    fn theme_table_form_captures_known_colour_keys() {
        let s = snapshot_after(
            r##"
            owlry.theme {
              background           = "#1e1e2e",
              background_secondary = "#313244",
              border               = "#45475a",
              text                 = "#cdd6f4",
              text_secondary       = "#a6adc8",
              accent               = "#cba6f7",
              accent_bright        = "#f5c2e7",
              badge_app            = "#a6e3a1",
              badge_power          = "#f38ba8",
              badge_uuctl          = "#9ece6a",
            }
            "##,
        );
        assert_eq!(s.theme_colors.background.as_deref(), Some("#1e1e2e"));
        assert_eq!(s.theme_colors.accent.as_deref(), Some("#cba6f7"));
        assert_eq!(s.theme_colors.badge_app.as_deref(), Some("#a6e3a1"));
        assert_eq!(s.theme_colors.badge_power.as_deref(), Some("#f38ba8"));
        assert_eq!(s.theme_colors.badge_uuctl.as_deref(), Some("#9ece6a"));
        assert!(s.unknown_theme_keys.is_empty());
    }

    #[test]
    fn theme_string_and_table_compose() {
        let s = snapshot_after(
            r##"
            owlry.theme("nord")
            owlry.theme { background = "#1e1e2e" }
            "##,
        );
        assert_eq!(s.theme_name.as_deref(), Some("nord"));
        assert_eq!(s.theme_colors.background.as_deref(), Some("#1e1e2e"));
    }

    #[test]
    fn theme_table_calls_merge_per_colour_key() {
        let s = snapshot_after(
            r##"
            owlry.theme { background = "#000000", accent = "#ff00ff" }
            owlry.theme { accent = "#00ffff" }
            "##,
        );
        assert_eq!(s.theme_colors.background.as_deref(), Some("#000000"));
        assert_eq!(s.theme_colors.accent.as_deref(), Some("#00ffff"));
    }

    #[test]
    fn theme_badge_sys_alias_maps_to_badge_power() {
        let s = snapshot_after(r##"owlry.theme { badge_sys = "#ff8800" }"##);
        assert_eq!(s.theme_colors.badge_power.as_deref(), Some("#ff8800"));
    }

    #[test]
    fn theme_unknown_keys_recorded_without_failing() {
        let s = snapshot_after(
            r##"
            owlry.theme {
              background = "#000000",
              future_v22_key = "#ffffff",
              another_unknown = "x",
            }
            "##,
        );
        assert_eq!(s.theme_colors.background.as_deref(), Some("#000000"));
        assert!(s.unknown_theme_keys.contains(&"future_v22_key".to_string()));
        assert!(
            s.unknown_theme_keys
                .contains(&"another_unknown".to_string())
        );
    }

    #[test]
    fn theme_wrong_argument_type_errors() {
        let ctx = LuaContext::new().expect("must build");
        let err = ctx.eval_str(r#"owlry.theme(42)"#).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("expected a string") || msg.contains("table"),
            "should mention expected shapes; got: {}",
            msg
        );
    }

    // ── owlry.profiles { ... } end-to-end ───────────────────────────────────

    #[test]
    fn profiles_captures_multiple_named_sets() {
        let s = snapshot_after(
            r#"
            owlry.profiles {
              dev     = { "app", "cmd", "ssh" },
              media   = { "emoji", "clipboard" },
              minimal = { "app" },
            }
            "#,
        );
        let map = s.profiles.expect("profiles must be Some");
        assert_eq!(map.len(), 3);
        assert_eq!(
            map.get("dev"),
            Some(&vec![
                "app".to_string(),
                "cmd".to_string(),
                "ssh".to_string()
            ])
        );
        assert_eq!(map.get("minimal"), Some(&vec!["app".to_string()]));
    }

    #[test]
    fn profiles_called_twice_replaces_the_map() {
        let s = snapshot_after(
            r#"
            owlry.profiles { dev = { "app", "cmd" } }
            owlry.profiles { media = { "emoji" } }
            "#,
        );
        let map = s.profiles.expect("profiles must be Some");
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("media"));
        assert!(!map.contains_key("dev"), "second call must fully replace");
    }

    #[test]
    fn profiles_with_non_list_value_errors() {
        let ctx = LuaContext::new().expect("must build");
        let err = ctx
            .eval_str(r#"owlry.profiles { broken = "not a list" }"#)
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("owlry.profiles") || msg.contains("list"),
            "should indicate list-of-strings expectation; got: {}",
            msg
        );
    }

    #[test]
    fn owlry_util_is_attached_to_owlry_table() {
        // owlry.util.* lands via api::install — verify the sub-table exists
        // and a representative function is callable end-to-end.
        let ctx = LuaContext::new().expect("must build");
        ctx.eval_str(
            r#"
            assert(type(owlry.util) == "table", "owlry.util must be a table")
            assert(type(owlry.util.shell) == "function", "owlry.util.shell must be a function")
            assert(type(owlry.util.shell_lines) == "function")
            assert(type(owlry.util.read_file) == "function")
            assert(type(owlry.util.glob) == "function")
            assert(type(owlry.util.env) == "function")
            assert(type(owlry.util.hostname) == "function")
            assert(owlry.util.shell("echo end-to-end") == "end-to-end")
            "#,
        )
        .expect("util surface must be live");
    }

    #[test]
    fn util_helpers_can_feed_into_user_provider_items() {
        // Realistic shape from docs/lua-api.md §3: build items via shell.
        let ctx = LuaContext::new().expect("must build");
        ctx.eval_str(
            r#"
            owlry.provider {
              id = "hosts",
              items = function()
                local h = owlry.util.hostname()
                return {
                  { name = "current host: " .. h, command = "true" },
                }
              end,
            }
            "#,
        )
        .expect("registration must succeed");
        let cfg = ctx.snapshot();
        assert_eq!(cfg.user_providers.len(), 1);
        // Build the LuaProvider, refresh, and assert hostname appeared.
        use crate::lua::LuaProvider;
        use crate::providers::Provider;
        let mut p = LuaProvider::new(cfg.user_providers[0].clone(), ctx.lua_handle());
        p.refresh();
        let items = p.items();
        assert_eq!(items.len(), 1);
        assert!(items[0].name.starts_with("current host: "));
        assert!(items[0].name.len() > "current host: ".len());
    }

    #[test]
    fn profiles_coerce_numeric_entries_via_lua_tostring() {
        // mlua's String::from_lua follows Lua semantics (tostring coercion),
        // so `{ "app", 42 }` becomes ["app", "42"]. Document the behaviour
        // here so it isn't accidentally tightened — config validate (3.9)
        // can flag "42" as an unknown provider id if needed.
        let s = snapshot_after(r#"owlry.profiles { dev = { "app", 42 } }"#);
        let map = s.profiles.expect("profiles must be Some");
        assert_eq!(
            map.get("dev"),
            Some(&vec!["app".to_string(), "42".to_string()])
        );
    }
}
