//! `owlry config validate` for the Lua side per docs/lua-api.md §8.
//!
//! Categories (with exit codes from spec §8):
//! - **errors** (exit 1) — syntax / eval failures. Returned to the caller
//!   as the `LuaConfigError` from `LoadedConfig::load_lua_path`; we don't
//!   surface them through [`ValidationReport`] because there's nothing
//!   else to check once eval failed.
//! - **warnings** (exit 2) — unknown keys, unknown ids, tabs ⊄ providers,
//!   duplicate provider ids, providers compiled out.
//! - **clean** (exit 0) — no warnings.
//!
//! The validator is a pure function over a [`LuaConfig`] snapshot. The
//! provider-id and theme-key tables it consults are the same canonical
//! sets used by [`super::config::apply_providers_list`] and
//! [`super::config::KNOWN_THEME_KEYS`], so a key/id accepted by the
//! merger is never flagged by the validator (and vice-versa).

use super::config::{KNOWN_THEME_KEYS, LuaConfig};

/// Categorised findings from validating an `owlry.lua` snapshot.
#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationReport {
    /// Spec §8 exit codes: 0 clean, 1 errors, 2 warnings-only.
    pub fn exit_code(&self) -> i32 {
        if !self.errors.is_empty() {
            1
        } else if !self.warnings.is_empty() {
            2
        } else {
            0
        }
    }

    pub fn is_clean(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }
}

/// Inspect a [`LuaConfig`] snapshot and produce a [`ValidationReport`].
pub fn validate(cfg: &LuaConfig) -> ValidationReport {
    let mut report = ValidationReport::default();
    let user_ids: Vec<&str> = cfg.user_providers.iter().map(|s| s.id.as_str()).collect();

    // owlry.set — unknown keys (recorded at eval time).
    for key in &cfg.unknown_settings {
        report.warnings.push(format!(
            "owlry.set: unknown key `{}` — ignored (forward-compat for 2.2+ keys)",
            key
        ));
    }

    // owlry.theme — unknown keys (recorded at eval time).
    for key in &cfg.unknown_theme_keys {
        // KNOWN_THEME_KEYS contains the pre-v2 `badge_sys` alias too; the
        // tracker already filters those, so any leftover key is genuinely
        // unrecognised.
        if !KNOWN_THEME_KEYS.contains(&key.as_str()) {
            report.warnings.push(format!(
                "owlry.theme: unknown colour key `{}` — ignored",
                key
            ));
        }
    }

    // owlry.providers — unknown ids and compiled-out warnings.
    if let Some(list) = &cfg.providers {
        for id in list {
            match canonical_provider_id(id) {
                Some(canon) => {
                    if !is_compiled_in(canon) {
                        report.warnings.push(format!(
                            "owlry.providers: `{}` is not compiled into this build — \
                             it will be silently dropped at runtime. Rebuild owlry \
                             with the matching cargo feature to enable it.",
                            id
                        ));
                    }
                }
                None => {
                    if !user_ids.contains(&id.as_str()) {
                        report.warnings.push(format!(
                            "owlry.providers: unknown id `{}` — not a built-in and not \
                             registered by any `owlry.provider {{ id = … }}` call",
                            id
                        ));
                    }
                }
            }
        }
    }

    // owlry.tabs — unknown ids; entries not in providers list.
    if let Some(tabs) = &cfg.tabs {
        let enabled_canon: Option<std::collections::HashSet<&str>> = cfg
            .providers
            .as_ref()
            .map(|v| v.iter().filter_map(|s| canonical_provider_id(s)).collect());
        for id in tabs {
            let canon = canonical_provider_id(id);
            match canon {
                Some(c) => {
                    if let Some(ref set) = enabled_canon
                        && !set.contains(c)
                    {
                        report.warnings.push(format!(
                            "owlry.tabs: `{}` is not in owlry.providers — it will \
                             be dropped from the tab bar",
                            id
                        ));
                    }
                }
                None => {
                    if !user_ids.contains(&id.as_str()) {
                        report.warnings.push(format!(
                            "owlry.tabs: unknown id `{}` — not a built-in and not \
                             a user-defined provider",
                            id
                        ));
                    }
                }
            }
        }
    }

    // owlry.provider duplicates.
    for dup in &cfg.duplicate_user_provider_ids {
        report.warnings.push(format!(
            "owlry.provider: id `{}` was registered more than once — the last \
             definition wins (earlier registrations are dropped)",
            dup
        ));
    }

    // owlry.profiles — unknown ids inside each profile's mode list.
    if let Some(profiles) = &cfg.profiles {
        let mut profile_names: Vec<&String> = profiles.keys().collect();
        profile_names.sort();
        for name in profile_names {
            for id in &profiles[name] {
                let known = canonical_provider_id(id).is_some() || user_ids.contains(&id.as_str());
                if !known {
                    report.warnings.push(format!(
                        "owlry.profiles.{}: unknown id `{}` — not a built-in and \
                         not a user-defined provider",
                        name, id
                    ));
                }
            }
        }
    }

    report
}

/// Resolve a Lua-side provider id to its canonical form. Mirrors the
/// alias table in [`super::config::apply_providers_list`].
fn canonical_provider_id(id: &str) -> Option<&'static str> {
    match id {
        "app" | "application" | "applications" => Some("applications"),
        "cmd" | "command" | "commands" => Some("commands"),
        "calc" | "calculator" => Some("calculator"),
        "conv" | "converter" => Some("converter"),
        "power" | "sys" | "system" => Some("power"),
        "systemd" | "uuctl" => Some("systemd"),
        "ssh" => Some("ssh"),
        "clipboard" | "clip" => Some("clipboard"),
        "emoji" => Some("emoji"),
        "filesearch" | "file" => Some("filesearch"),
        "websearch" | "web" | "search" => Some("websearch"),
        "dmenu" => Some("dmenu"),
        _ => None,
    }
}

/// Whether the canonical provider id is compiled into the current build.
/// Always-compiled-in ids (applications, commands, calc, conv, power,
/// dmenu) return `true` unconditionally; the rest gate on cargo features.
fn is_compiled_in(canonical: &str) -> bool {
    match canonical {
        "applications" | "commands" | "calculator" | "converter" | "power" | "dmenu" => true,
        "clipboard" => cfg!(feature = "clipboard"),
        "emoji" => cfg!(feature = "emoji"),
        "filesearch" => cfg!(feature = "filesearch"),
        "ssh" => cfg!(feature = "ssh"),
        "systemd" => cfg!(feature = "systemd"),
        "websearch" => cfg!(feature = "websearch"),
        _ => true, // unknown canonical → not our call
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeColors;
    use std::collections::HashMap;

    fn empty_cfg() -> LuaConfig {
        LuaConfig::default()
    }

    #[test]
    fn clean_config_produces_no_findings() {
        let report = validate(&empty_cfg());
        assert!(report.is_clean());
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn unknown_set_key_becomes_warning() {
        let mut cfg = empty_cfg();
        cfg.unknown_settings.push("mystery_2_2_key".into());
        let report = validate(&cfg);
        assert_eq!(report.exit_code(), 2);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("mystery_2_2_key")),
            "got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn unknown_theme_key_becomes_warning() {
        let mut cfg = empty_cfg();
        cfg.unknown_theme_keys.push("future_palette_token".into());
        let report = validate(&cfg);
        assert_eq!(report.exit_code(), 2);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("future_palette_token"))
        );
    }

    #[test]
    fn unknown_provider_id_becomes_warning() {
        let mut cfg = empty_cfg();
        cfg.providers = Some(vec!["app".into(), "fictional".into()]);
        let report = validate(&cfg);
        assert_eq!(report.exit_code(), 2);
        assert!(
            report.warnings.iter().any(|w| w.contains("fictional")),
            "got: {:?}",
            report.warnings
        );
        // `app` is recognised and must NOT produce a warning.
        assert!(!report.warnings.iter().any(|w| w.contains("`app`")));
    }

    #[test]
    fn user_provider_id_in_providers_list_is_known() {
        let mut cfg = empty_cfg();
        cfg.providers = Some(vec!["hs".into()]);
        // Pretend `hs` was registered as a user provider.
        cfg.user_providers.push(make_spec("hs"));
        let report = validate(&cfg);
        assert!(report.is_clean(), "got: {:?}", report);
    }

    #[test]
    fn tabs_unknown_id_becomes_warning() {
        let mut cfg = empty_cfg();
        cfg.tabs = Some(vec!["app".into(), "ghost".into()]);
        let report = validate(&cfg);
        assert!(
            report.warnings.iter().any(|w| w.contains("ghost")),
            "got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn tabs_entry_not_in_providers_list_is_warning() {
        let mut cfg = empty_cfg();
        cfg.providers = Some(vec!["app".into()]);
        cfg.tabs = Some(vec!["app".into(), "cmd".into()]); // cmd not enabled
        let report = validate(&cfg);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("cmd") && w.contains("not in owlry.providers")),
            "got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn tabs_user_provider_outside_providers_list_is_ok() {
        // User providers auto-join the enabled set, so they don't need to
        // appear in owlry.providers. Listing them in tabs should be fine.
        let mut cfg = empty_cfg();
        cfg.providers = Some(vec!["app".into()]);
        cfg.tabs = Some(vec!["app".into(), "hs".into()]);
        cfg.user_providers.push(make_spec("hs"));
        let report = validate(&cfg);
        assert!(report.is_clean(), "got: {:?}", report);
    }

    #[test]
    fn duplicate_provider_id_becomes_warning() {
        let mut cfg = empty_cfg();
        cfg.duplicate_user_provider_ids.push("hs".into());
        let report = validate(&cfg);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("registered more than once")),
            "got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn profile_with_unknown_id_warns_per_id() {
        let mut cfg = empty_cfg();
        let mut p: HashMap<String, Vec<String>> = HashMap::new();
        p.insert("dev".into(), vec!["app".into(), "phantom".into()]);
        cfg.profiles = Some(p);
        let report = validate(&cfg);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("owlry.profiles.dev") && w.contains("phantom")),
            "got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn pre_v2_aliases_are_known() {
        // `sys` and `uuctl` resolve to power and systemd respectively.
        // We only assert they're NOT reported as unknown ids — a
        // compiled-out warning is environment-dependent (running this
        // with `--no-default-features --features lua` omits the systemd
        // feature, which legitimately flags uuctl as compiled-out).
        let mut cfg = empty_cfg();
        cfg.providers = Some(vec!["sys".into(), "uuctl".into()]);
        cfg.tabs = Some(vec!["sys".into(), "uuctl".into()]);
        let report = validate(&cfg);
        let unknown_count = report
            .warnings
            .iter()
            .filter(|w| w.contains("unknown id"))
            .count();
        assert_eq!(
            unknown_count, 0,
            "pre-v2 aliases must not be flagged as unknown ids; got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn compiled_out_provider_is_warning_when_features_dropped() {
        // We can't easily flip features at test time, but we can verify
        // that ALWAYS-compiled-in ids never produce a "not compiled in"
        // warning — that locks in the alias table from regressing.
        let mut cfg = empty_cfg();
        cfg.providers = Some(vec![
            "app".into(),
            "cmd".into(),
            "calc".into(),
            "conv".into(),
            "power".into(),
        ]);
        let report = validate(&cfg);
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| w.contains("not compiled into this build")),
            "always-on providers must never warn about compile-out; got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn report_with_multiple_findings_keeps_them_all() {
        let mut cfg = empty_cfg();
        cfg.unknown_settings.push("a".into());
        cfg.unknown_theme_keys.push("b".into());
        cfg.providers = Some(vec!["nope".into()]);
        cfg.duplicate_user_provider_ids.push("hs".into());
        let report = validate(&cfg);
        // 4 categories → at least 4 warnings.
        assert!(
            report.warnings.len() >= 4,
            "expected several warnings, got {}: {:?}",
            report.warnings.len(),
            report.warnings
        );
        assert_eq!(report.exit_code(), 2);
    }

    fn make_spec(id: &str) -> super::super::config::LuaProviderSpec {
        // Build a minimal LuaProviderSpec for testing. We can't easily
        // synthesize an mlua::Function without a Lua context, so we
        // borrow one from a throwaway state.
        use mlua::Lua;
        let lua = Lua::new();
        let f = lua
            .create_function(|_, ()| Ok(Vec::<mlua::Table>::new()))
            .unwrap();
        super::super::config::LuaProviderSpec {
            id: id.to_string(),
            name: None,
            prefix: None,
            tab_label: None,
            icon: None,
            search_noun: None,
            priority: 0,
            dynamic: false,
            items_fn: f,
        }
    }

    // ThemeColors / ProfileConfig usage references just to silence
    // unused-import warnings when the imports above aren't otherwise hit.
    #[allow(dead_code)]
    fn _refs() {
        let _ = ThemeColors::default();
    }
}
