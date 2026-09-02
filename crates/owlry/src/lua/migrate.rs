//! TOML → `owlry.lua` migration per docs/lua-api.md §9.
//!
//! Reads the user's `config.toml`, parses it through the existing serde
//! pipeline (so pre-v2 aliases like `system`/`badge_sys` already
//! normalise to v2 names), and emits an equivalent `owlry.lua`.
//!
//! Properties guaranteed:
//! - **Deterministic**: every list / map is alphabetically sorted, so
//!   running the migrator twice on the same TOML produces byte-identical
//!   Lua. Tested in [`tests::migration_is_deterministic`].
//! - **Minimal**: only values that differ from `Config::default()` are
//!   emitted. A TOML file with no explicit settings produces a Lua
//!   stub of just the header comment.
//! - **Round-trippable**: the generated Lua, evaluated and merged onto
//!   a default Config, must match the source Config's scalar fields.
//!   Tested via [`tests::round_trip_preserves_scalars`].

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("no TOML config to migrate at {0}")]
    NoToml(PathBuf),
    #[error("destination already exists: {0} — pass --force to overwrite")]
    DestExists(PathBuf),
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("failed to write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// What the caller asks the migrator to do.
pub struct MigrateRequest<'a> {
    pub toml_path: &'a Path,
    pub lua_path: &'a Path,
    pub force: bool,
}

/// Summary written to stdout by `owlry migrate-config`.
#[derive(Debug)]
pub struct MigrateOutcome {
    pub lua_path: PathBuf,
    pub bytes_written: usize,
    /// Pre-v2 artifacts that survived in the config dir but can't be
    /// auto-migrated — typically C-ABI / Rune plugins from `plugins/`
    /// and scripts from the (now-removed) `scripts/` provider. The
    /// command handler surfaces these to the user as info notes.
    pub legacy_artifacts: Vec<LegacyArtifact>,
}

/// A piece of pre-v2 user content that the migrator detected but didn't
/// translate — the user has to rewrite it (or delete it) by hand.
#[derive(Debug, Clone)]
pub struct LegacyArtifact {
    pub path: PathBuf,
    pub kind: LegacyKind,
}

#[derive(Debug, Clone)]
pub enum LegacyKind {
    /// A pre-v2 plugin directory (Rune / C-ABI). The plugin.toml fields
    /// are surfaced verbatim so the user can paste them into a new
    /// `owlry.provider { ... }` block.
    Plugin {
        id: String,
        name: Option<String>,
        prefix: Option<String>,
        icon: Option<String>,
        entry_point: Option<String>,
    },
    /// The pre-v2 `scripts/` directory (the standalone `scripts` provider
    /// is gone in 2.0; the contents are now user data with no automatic
    /// surface).
    ScriptsDir { entries: Vec<String> },
}

/// Run the full migration: validate paths, parse TOML, emit Lua, write file.
pub fn migrate(req: &MigrateRequest<'_>) -> Result<MigrateOutcome, MigrateError> {
    if !req.toml_path.exists() {
        return Err(MigrateError::NoToml(req.toml_path.to_path_buf()));
    }
    if req.lua_path.exists() && !req.force {
        return Err(MigrateError::DestExists(req.lua_path.to_path_buf()));
    }

    let raw_toml = std::fs::read_to_string(req.toml_path).map_err(|e| MigrateError::Read {
        path: req.toml_path.to_path_buf(),
        source: e,
    })?;
    let cfg = Config::load_from_toml(Some(req.toml_path)).map_err(|e| MigrateError::Parse {
        path: req.toml_path.to_path_buf(),
        message: e.to_string(),
    })?;

    let lua = generate_lua(&cfg, &raw_toml, req.toml_path);

    if let Some(parent) = req.lua_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| MigrateError::Write {
            path: req.lua_path.to_path_buf(),
            source: e,
        })?;
    }
    std::fs::write(req.lua_path, &lua).map_err(|e| MigrateError::Write {
        path: req.lua_path.to_path_buf(),
        source: e,
    })?;

    // Sniff for pre-v2 artifacts in the config dir (plugins/, scripts/).
    // These don't block migration — we just surface them so the user
    // doesn't silently lose what used to be working providers.
    let config_dir = req.toml_path.parent().unwrap_or_else(|| Path::new("."));
    let legacy_artifacts = scan_legacy_artifacts(config_dir);

    Ok(MigrateOutcome {
        lua_path: req.lua_path.to_path_buf(),
        bytes_written: lua.len(),
        legacy_artifacts,
    })
}

/// Inspect `config_dir` for pre-v2 user data that the migrator can't
/// auto-translate: `plugins/<name>/` (Rune / C-ABI) and `scripts/`
/// (the legacy `scripts` provider). Returns an empty Vec when nothing
/// is found, so callers can `if !empty` and short-circuit.
pub fn scan_legacy_artifacts(config_dir: &Path) -> Vec<LegacyArtifact> {
    let mut out = Vec::new();

    let plugins_dir = config_dir.join("plugins");
    if plugins_dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&plugins_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            // Skip dotfile dirs (`.claude`, `.git`, `.cache`, etc.) and any
            // directory without a `plugin.toml`. The pre-v2 plugin format
            // *always* shipped a manifest — anything without one is just
            // user data that happens to live under `plugins/`.
            .filter(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') {
                    return false;
                }
                e.path().join("plugin.toml").is_file()
            })
            .collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            let kind = parse_plugin_dir(&path);
            out.push(LegacyArtifact { path, kind });
        }
    }

    let scripts_dir = config_dir.join("scripts");
    if scripts_dir.is_dir() {
        let mut entries: Vec<String> = std::fs::read_dir(&scripts_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        out.push(LegacyArtifact {
            path: scripts_dir,
            kind: LegacyKind::ScriptsDir { entries },
        });
    }

    out
}

/// Best-effort parse of a `plugin.toml` inside a pre-v2 plugin dir.
/// Falls back to using the directory name as the id when the toml is
/// missing or malformed — the goal is informational, not authoritative.
fn parse_plugin_dir(dir: &Path) -> LegacyKind {
    let dir_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "?".into());

    let manifest_path = dir.join("plugin.toml");
    let manifest_str = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(_) => {
            return LegacyKind::Plugin {
                id: dir_name,
                name: None,
                prefix: None,
                icon: None,
                entry_point: None,
            };
        }
    };

    let parsed: toml::Value = match toml::from_str(&manifest_str) {
        Ok(v) => v,
        Err(_) => {
            return LegacyKind::Plugin {
                id: dir_name,
                name: None,
                prefix: None,
                icon: None,
                entry_point: None,
            };
        }
    };

    // The pre-v2 manifest format had [plugin] (id/name/entry_point) and
    // [[providers]] (id/name/prefix/icon). The first provider's prefix
    // and icon are the most useful for a user trying to recreate it.
    let plugin = parsed.get("plugin");
    let providers = parsed.get("providers").and_then(|v| v.as_array());
    let first_provider = providers.and_then(|arr| arr.first());

    let id = plugin
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or(dir_name);
    let name = plugin
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let entry_point = plugin
        .and_then(|p| p.get("entry_point"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let prefix = first_provider
        .and_then(|p| p.get("prefix"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let icon = first_provider
        .and_then(|p| p.get("icon"))
        .and_then(|v| v.as_str())
        .map(String::from);

    LegacyKind::Plugin {
        id,
        name,
        prefix,
        icon,
        entry_point,
    }
}

/// Produce the Lua text for a parsed Config. The `raw_toml` and `src`
/// are only used to render the header comment block — they don't drive
/// any field emission. Pure function: same input → same output bytes.
pub fn generate_lua(cfg: &Config, raw_toml: &str, src: &Path) -> String {
    let mut out = String::new();

    // Header: file provenance + any leading `#` comment block from the
    // source TOML (survives the round-trip since serde-derived TOML
    // parsing throws everything else away).
    out.push_str("-- Generated by `owlry migrate-config` from ");
    out.push_str(&src.display().to_string());
    out.push_str(".\n");
    out.push_str("-- Edit freely; this file is the canonical owlry config from 2.1 onwards.\n");
    out.push_str("-- Spec: https://somegit.dev/Owlibou/owlry/src/branch/main/docs/lua-api.md\n");
    let header_lines = extract_header_comments(raw_toml);
    if !header_lines.is_empty() {
        out.push_str("--\n-- Preserved from your config.toml:\n");
        for line in header_lines {
            out.push_str("-- ");
            out.push_str(line.trim_start_matches('#').trim_start());
            out.push('\n');
        }
    }
    out.push('\n');

    emit_set_section(&mut out, cfg);
    emit_tabs_section(&mut out, cfg);
    emit_providers_section(&mut out, cfg);
    emit_theme_section(&mut out, cfg);
    emit_profiles_section(&mut out, cfg);

    out
}

// =============================================================================
// Section emitters
// =============================================================================

fn emit_set_section(out: &mut String, cfg: &Config) {
    let d = Config::default();
    let mut entries: Vec<(&'static str, String)> = Vec::new();

    if cfg.appearance.theme != d.appearance.theme
        && let Some(ref v) = cfg.appearance.theme
    {
        entries.push(("theme", lua_string(v)));
    }
    if cfg.appearance.width != d.appearance.width {
        entries.push(("width", cfg.appearance.width.to_string()));
    }
    if cfg.appearance.height != d.appearance.height {
        entries.push(("height", cfg.appearance.height.to_string()));
    }
    if cfg.appearance.font_size != d.appearance.font_size {
        entries.push(("font_size", cfg.appearance.font_size.to_string()));
    }
    if cfg.appearance.border_radius != d.appearance.border_radius {
        entries.push(("border_radius", cfg.appearance.border_radius.to_string()));
    }
    // terminal: only emit if explicitly set in TOML. The TOML load path
    // auto-detects when None, but we only want to emit user intent here.
    // We can't distinguish "user set it" from "auto-detect filled it"
    // post-load; settle for: emit iff different from None AND from the
    // auto-detection chain's heuristic-free default ("xterm").
    if let Some(ref t) = cfg.general.terminal_command
        && t != "xterm"
    {
        entries.push(("terminal", lua_string(t)));
    }
    if cfg.general.use_uwsm != d.general.use_uwsm {
        entries.push(("use_uwsm", cfg.general.use_uwsm.to_string()));
    }
    if cfg.general.show_icons != d.general.show_icons {
        entries.push(("show_icons", cfg.general.show_icons.to_string()));
    }
    if cfg.general.max_results != d.general.max_results {
        entries.push(("max_results", cfg.general.max_results.to_string()));
    }
    if cfg.providers.frecency != d.providers.frecency {
        entries.push(("frecency", cfg.providers.frecency.to_string()));
    }
    if (cfg.providers.frecency_weight - d.providers.frecency_weight).abs() > f64::EPSILON {
        entries.push((
            "frecency_weight",
            format!("{}", cfg.providers.frecency_weight),
        ));
    }
    if cfg.providers.frecency_max_entries != d.providers.frecency_max_entries {
        entries.push((
            "frecency_max_entries",
            cfg.providers.frecency_max_entries.to_string(),
        ));
    }
    if cfg.providers.frecency_max_age_days != d.providers.frecency_max_age_days {
        entries.push((
            "frecency_max_age_days",
            cfg.providers.frecency_max_age_days.to_string(),
        ));
    }
    if cfg.providers.frecency_min_launches != d.providers.frecency_min_launches {
        entries.push((
            "frecency_min_launches",
            cfg.providers.frecency_min_launches.to_string(),
        ));
    }
    if cfg.providers.search_engine != d.providers.search_engine {
        entries.push(("search_engine", lua_string(&cfg.providers.search_engine)));
    }
    if cfg.providers.filesearch_roots != d.providers.filesearch_roots {
        let items: Vec<String> = cfg
            .providers
            .filesearch_roots
            .iter()
            .map(|r| lua_string(r))
            .collect();
        entries.push(("filesearch_roots", format!("{{ {} }}", items.join(", "))));
    }

    if entries.is_empty() {
        return;
    }

    out.push_str("-- Global settings (only values differing from defaults).\n");
    out.push_str("owlry.set {\n");
    for (k, v) in entries {
        out.push_str("  ");
        out.push_str(k);
        out.push_str(" = ");
        out.push_str(&v);
        out.push_str(",\n");
    }
    out.push_str("}\n\n");
}

fn emit_tabs_section(out: &mut String, cfg: &Config) {
    let default_tabs = Config::default().general.tabs;
    if cfg.general.tabs == default_tabs {
        return;
    }
    out.push_str("-- Tab bar order.\n");
    out.push_str("owlry.tabs { ");
    for (i, t) in cfg.general.tabs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&lua_string(t));
    }
    out.push_str(" }\n\n");
}

fn emit_providers_section(out: &mut String, cfg: &Config) {
    // Build the enabled list using the canonical id for each entry.
    let mut enabled: Vec<&'static str> = Vec::new();
    if cfg.providers.applications {
        enabled.push("app");
    }
    if cfg.providers.commands {
        enabled.push("cmd");
    }
    if cfg.providers.calculator {
        enabled.push("calc");
    }
    if cfg.providers.converter {
        enabled.push("conv");
    }
    if cfg.providers.power {
        enabled.push("power");
    }
    if cfg.providers.systemd {
        enabled.push("systemd");
    }
    if cfg.providers.clipboard {
        enabled.push("clipboard");
    }
    if cfg.providers.emoji {
        enabled.push("emoji");
    }
    if cfg.providers.rofimoji {
        enabled.push("rofimoji");
    }
    if cfg.providers.filesearch {
        enabled.push("filesearch");
    }
    if cfg.providers.ssh {
        enabled.push("ssh");
    }
    if cfg.providers.websearch {
        enabled.push("websearch");
    }

    // Skip emission if all providers are enabled (the default). Lua
    // semantics treat "no owlry.providers call" identically to "all
    // built-ins enabled", so omitting keeps the output minimal.
    if enabled.len() == 12 {
        return;
    }

    enabled.sort_unstable();
    out.push_str(
        "-- Enabled providers (omit IDs to disable them; built-ins not listed are off).\n",
    );
    out.push_str("owlry.providers {\n");
    for id in enabled {
        out.push_str("  ");
        out.push_str(&lua_string(id));
        out.push_str(",\n");
    }
    out.push_str("}\n\n");
}

fn emit_theme_section(out: &mut String, cfg: &Config) {
    // Two pieces: name (string-form theme call) and colour overrides
    // (table-form theme call). Per spec §4.5 they compose, so we emit
    // both when both are set.
    let has_name = cfg.appearance.theme.is_some();
    let colours = collect_set_colours(cfg);
    if !has_name && colours.is_empty() {
        return;
    }

    // The `theme = ...` scalar already lives in owlry.set, but the
    // string-form owlry.theme(name) is the §4.5-blessed surface. Emit
    // theme(name) here too only if NOT already in owlry.set — i.e. we
    // don't double-emit. Currently emit_set already handles theme via
    // the scalar key, so we skip the name form here to keep the output
    // single-sourced.

    if !colours.is_empty() {
        out.push_str("-- Theme colour overrides.\n");
        out.push_str("owlry.theme {\n");
        for (key, value) in colours {
            out.push_str("  ");
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&lua_string(&value));
            out.push_str(",\n");
        }
        out.push_str("}\n\n");
    }
}

fn collect_set_colours(cfg: &Config) -> Vec<(&'static str, String)> {
    let c = &cfg.appearance.colors;
    macro_rules! push_if_set {
        ($vec:ident, $cfg_field:ident, $name:expr) => {
            if let Some(ref v) = c.$cfg_field {
                $vec.push(($name, v.clone()));
            }
        };
    }
    let mut v = Vec::new();
    push_if_set!(v, background, "background");
    push_if_set!(v, background_secondary, "background_secondary");
    push_if_set!(v, border, "border");
    push_if_set!(v, text, "text");
    push_if_set!(v, text_secondary, "text_secondary");
    push_if_set!(v, accent, "accent");
    push_if_set!(v, accent_bright, "accent_bright");
    push_if_set!(v, badge_app, "badge_app");
    push_if_set!(v, badge_bookmark, "badge_bookmark");
    push_if_set!(v, badge_calc, "badge_calc");
    push_if_set!(v, badge_clip, "badge_clip");
    push_if_set!(v, badge_cmd, "badge_cmd");
    push_if_set!(v, badge_dmenu, "badge_dmenu");
    push_if_set!(v, badge_emoji, "badge_emoji");
    push_if_set!(v, badge_file, "badge_file");
    push_if_set!(v, badge_script, "badge_script");
    push_if_set!(v, badge_ssh, "badge_ssh");
    push_if_set!(v, badge_power, "badge_power");
    push_if_set!(v, badge_uuctl, "badge_uuctl");
    push_if_set!(v, badge_web, "badge_web");
    push_if_set!(v, badge_media, "badge_media");
    push_if_set!(v, badge_weather, "badge_weather");
    push_if_set!(v, badge_pomo, "badge_pomo");
    v.sort_by_key(|(k, _)| *k);
    v
}

fn emit_profiles_section(out: &mut String, cfg: &Config) {
    if cfg.profiles.is_empty() {
        return;
    }

    out.push_str("-- Named profiles selected by `owlry --profile <name>`.\n");
    out.push_str("owlry.profiles {\n");
    let mut names: Vec<&String> = cfg.profiles.keys().collect();
    names.sort();
    for name in names {
        let profile = &cfg.profiles[name];
        out.push_str("  ");
        emit_table_key(out, name);
        // Bare list unless the profile carries a non-default option, which
        // needs the `providers = { … }` table form to hang the option off.
        out.push_str(if profile.show_all_tab {
            " = { "
        } else {
            " = { providers = { "
        });
        for (i, m) in profile.modes.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&lua_string(m));
        }
        out.push_str(if profile.show_all_tab {
            " },\n"
        } else {
            " }, show_all_tab = false },\n"
        });
    }
    out.push_str("}\n\n");
}

/// Emit a Lua table key — bare identifier for valid names, `["…"]`
/// bracket form for anything that isn't (`my-profile`, names with
/// spaces, names starting with a digit, reserved words). Without this,
/// `owlry.profiles { my-profile = { ... } }` would be a Lua syntax error.
fn emit_table_key(out: &mut String, name: &str) {
    if is_valid_lua_identifier(name) {
        out.push_str(name);
    } else {
        out.push('[');
        out.push_str(&lua_string(name));
        out.push(']');
    }
}

fn is_valid_lua_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    // Reserved words in Lua 5.4. Bracket-quote them to be safe.
    !matches!(
        s,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "goto"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

// =============================================================================
// Helpers
// =============================================================================

/// Render a Rust string as a Lua double-quoted string with `\` and `"`
/// escaped. Non-ASCII bytes are passed through; Lua 5.4 strings are
/// 8-bit clean.
fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Pull the leading comment block from a TOML file — consecutive lines
/// starting with `#` (or empty) at the very top. Stops at the first
/// non-comment, non-empty line.
fn extract_header_comments(toml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in toml.lines() {
        let t = line.trim_start();
        if t.starts_with('#') {
            out.push(line.to_string());
        } else if t.is_empty() {
            if !out.is_empty() {
                out.push(line.to_string());
            }
        } else {
            break;
        }
    }
    // Strip a trailing blank line for tidiness.
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProfileConfig;
    use std::io::Write;

    fn write_toml(dir: &Path, contents: &str) -> PathBuf {
        let path = dir.join("config.toml");
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(contents.as_bytes()).expect("write");
        path
    }

    #[test]
    fn lua_string_escapes_special_chars() {
        assert_eq!(lua_string("plain"), "\"plain\"");
        assert_eq!(lua_string("with \"quotes\""), "\"with \\\"quotes\\\"\"");
        assert_eq!(lua_string("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(lua_string("new\nline"), "\"new\\nline\"");
        assert_eq!(lua_string("ütf8 — €"), "\"ütf8 — €\"");
    }

    #[test]
    fn header_comments_extracted_until_first_real_line() {
        let toml = "# top line\n# more text\n# third\n\n[providers]\napp = true\n";
        let lines = extract_header_comments(toml);
        assert_eq!(lines, vec!["# top line", "# more text", "# third"]);
    }

    #[test]
    fn default_config_emits_only_header() {
        let cfg = Config::default();
        let lua = generate_lua(&cfg, "", Path::new("/dev/null"));
        // No owlry.set, owlry.tabs, owlry.providers, owlry.theme, owlry.profiles.
        assert!(!lua.contains("owlry.set"));
        assert!(!lua.contains("owlry.tabs"));
        assert!(!lua.contains("owlry.providers"));
        assert!(!lua.contains("owlry.theme"));
        assert!(!lua.contains("owlry.profiles"));
        assert!(lua.contains("Generated by `owlry migrate-config`"));
    }

    #[test]
    fn set_emits_only_modified_scalars() {
        let mut cfg = Config::default();
        cfg.appearance.width = 1024;
        cfg.general.max_results = 50;
        // height stays at default — must not appear.
        let lua = generate_lua(&cfg, "", Path::new("/dev/null"));
        assert!(lua.contains("width = 1024"));
        assert!(lua.contains("max_results = 50"));
        assert!(!lua.contains("height"));
        assert!(!lua.contains("font_size"));
    }

    #[test]
    fn providers_section_omitted_when_all_enabled() {
        let cfg = Config::default(); // all true
        let lua = generate_lua(&cfg, "", Path::new("/dev/null"));
        assert!(!lua.contains("owlry.providers"));
    }

    #[test]
    fn providers_section_emits_alphabetically() {
        let mut cfg = Config::default();
        cfg.providers.ssh = false;
        cfg.providers.emoji = false;
        let lua = generate_lua(&cfg, "", Path::new("/dev/null"));
        let provider_section = lua
            .split("owlry.providers {")
            .nth(1)
            .expect("must have providers block");
        let pos_app = provider_section.find("\"app\"").expect("app");
        let pos_cmd = provider_section.find("\"cmd\"").expect("cmd");
        let pos_systemd = provider_section.find("\"systemd\"").expect("systemd");
        assert!(pos_app < pos_cmd && pos_cmd < pos_systemd);
        // ssh and emoji must be absent.
        assert!(!provider_section.contains("\"ssh\""));
        assert!(!provider_section.contains("\"emoji\""));
    }

    #[test]
    fn theme_colours_emit_alphabetically() {
        let mut cfg = Config::default();
        cfg.appearance.colors.accent = Some("#ff00ff".into());
        cfg.appearance.colors.background = Some("#000000".into());
        cfg.appearance.colors.badge_app = Some("#a6e3a1".into());
        let lua = generate_lua(&cfg, "", Path::new("/dev/null"));
        let section = lua.split("owlry.theme {").nth(1).expect("theme block");
        let pos_accent = section.find("accent =").unwrap();
        let pos_bg = section.find("background =").unwrap();
        let pos_badge = section.find("badge_app =").unwrap();
        assert!(pos_accent < pos_bg && pos_bg < pos_badge);
    }

    #[test]
    fn profiles_section_emits_alphabetically() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "minimal".into(),
            ProfileConfig {
                modes: vec!["app".into()],
                ..Default::default()
            },
        );
        cfg.profiles.insert(
            "dev".into(),
            ProfileConfig {
                modes: vec!["app".into(), "cmd".into(), "ssh".into()],
                ..Default::default()
            },
        );
        let lua = generate_lua(&cfg, "", Path::new("/dev/null"));
        let pos_dev = lua.find("dev = {").expect("dev");
        let pos_minimal = lua.find("minimal = {").expect("minimal");
        assert!(pos_dev < pos_minimal, "dev must come before minimal");
    }

    #[test]
    fn migration_is_deterministic() {
        // Run generate_lua twice on the same config; bytes must match.
        let mut cfg = Config::default();
        cfg.appearance.theme = Some("nord".into());
        cfg.providers.ssh = false;
        cfg.providers.emoji = false;
        cfg.appearance.colors.background = Some("#1e1e2e".into());
        cfg.profiles.insert(
            "dev".into(),
            ProfileConfig {
                modes: vec!["app".into(), "cmd".into()],
                ..Default::default()
            },
        );
        let a = generate_lua(&cfg, "", Path::new("/x"));
        let b = generate_lua(&cfg, "", Path::new("/x"));
        assert_eq!(a, b);
    }

    #[test]
    fn migrate_refuses_existing_dest_without_force() {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml_path = write_toml(dir.path(), "[providers]\napp = true\n");
        let lua_path = dir.path().join("owlry.lua");
        std::fs::write(&lua_path, "-- existing").expect("write existing");

        let req = MigrateRequest {
            toml_path: &toml_path,
            lua_path: &lua_path,
            force: false,
        };
        match migrate(&req) {
            Err(MigrateError::DestExists(p)) => assert_eq!(p, lua_path),
            other => panic!("expected DestExists, got {:?}", other),
        }
        // Existing file untouched.
        let after = std::fs::read_to_string(&lua_path).unwrap();
        assert_eq!(after, "-- existing");
    }

    #[test]
    fn migrate_overwrites_with_force() {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml_path = write_toml(dir.path(), "[appearance]\ntheme = \"nord\"\n");
        let lua_path = dir.path().join("owlry.lua");
        std::fs::write(&lua_path, "-- old").expect("write old");

        let req = MigrateRequest {
            toml_path: &toml_path,
            lua_path: &lua_path,
            force: true,
        };
        let outcome = migrate(&req).expect("migrate must succeed");
        assert_eq!(outcome.lua_path, lua_path);

        let after = std::fs::read_to_string(&lua_path).unwrap();
        assert!(after.contains(r#"theme = "nord""#));
        assert!(!after.contains("-- old"));
    }

    #[test]
    fn migrate_refuses_when_toml_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml_path = dir.path().join("does-not-exist.toml");
        let lua_path = dir.path().join("owlry.lua");
        let req = MigrateRequest {
            toml_path: &toml_path,
            lua_path: &lua_path,
            force: false,
        };
        match migrate(&req) {
            Err(MigrateError::NoToml(p)) => assert_eq!(p, toml_path),
            other => panic!("expected NoToml, got {:?}", other),
        }
    }

    #[test]
    fn pre_v2_aliases_normalise_to_v2_names() {
        // TOML uses pre-v2 names (`system` for power, `badge_sys` for badge_power).
        let dir = tempfile::tempdir().expect("tempdir");
        let toml_path = write_toml(
            dir.path(),
            r##"
[providers]
system = false

[appearance.colors]
badge_sys = "#ff8800"
"##,
        );
        let lua_path = dir.path().join("owlry.lua");
        let req = MigrateRequest {
            toml_path: &toml_path,
            lua_path: &lua_path,
            force: false,
        };
        migrate(&req).expect("migrate");
        let lua = std::fs::read_to_string(&lua_path).unwrap();
        // serde alias means parsed Config carries `power=false`, `badge_power=Some(...)`.
        // Emit must use v2 names.
        let providers = lua.split("owlry.providers {").nth(1).unwrap();
        assert!(
            !providers.contains("\"system\""),
            "must not emit pre-v2 'system'"
        );
        assert!(!providers.contains("\"sys\""));
        assert!(!providers.contains("\"power\""), "power was disabled");
        let theme = lua.split("owlry.theme {").nth(1).unwrap();
        assert!(theme.contains("badge_power ="), "must emit v2 badge_power");
        assert!(!theme.contains("badge_sys"));
    }

    #[test]
    fn profile_names_with_hyphens_get_bracket_quoted() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "my-dev".into(),
            ProfileConfig {
                modes: vec!["app".into()],
                ..Default::default()
            },
        );
        cfg.profiles.insert(
            "media center".into(),
            ProfileConfig {
                modes: vec!["emoji".into()],
                ..Default::default()
            },
        );
        let lua = generate_lua(&cfg, "", Path::new("/x"));
        assert!(
            lua.contains(r#"["my-dev"] = { "#),
            "hyphenated key must be bracket-quoted; got:\n{}",
            lua
        );
        assert!(
            lua.contains(r#"["media center"] = { "#),
            "spaced key must be bracket-quoted; got:\n{}",
            lua
        );
    }

    #[test]
    fn lua_reserved_words_as_keys_get_bracket_quoted() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "end".into(),
            ProfileConfig {
                modes: vec!["app".into()],
                ..Default::default()
            },
        );
        let lua = generate_lua(&cfg, "", Path::new("/x"));
        assert!(lua.contains(r#"["end"] = { "#));
    }

    #[test]
    fn round_trip_preserves_scalars() {
        // End-to-end: TOML → generate_lua → eval through LuaContext → merge
        // onto fresh Config → must match the original scalar fields.
        let mut original = Config::default();
        original.appearance.theme = Some("catppuccin-mocha".into());
        original.appearance.width = 900;
        original.general.max_results = 25;
        original.providers.frecency_weight = 0.5;
        original.providers.ssh = false;
        original.appearance.colors.accent = Some("#cba6f7".into());
        original.profiles.insert(
            "dev".into(),
            ProfileConfig {
                modes: vec!["app".into(), "cmd".into()],
                ..Default::default()
            },
        );

        let lua = generate_lua(&original, "", Path::new("/x"));

        let dir = tempfile::tempdir().expect("tempdir");
        let lua_path = dir.path().join("owlry.lua");
        std::fs::write(&lua_path, &lua).expect("write");

        let ctx = crate::lua::LuaContext::new().expect("ctx");
        ctx.eval_file(&lua_path).expect("eval must succeed");
        let snapshot = ctx.snapshot();
        let mut rebuilt = Config::default();
        snapshot.merge_into(&mut rebuilt);

        assert_eq!(rebuilt.appearance.theme, original.appearance.theme);
        assert_eq!(rebuilt.appearance.width, original.appearance.width);
        assert_eq!(rebuilt.general.max_results, original.general.max_results);
        assert!(
            (rebuilt.providers.frecency_weight - original.providers.frecency_weight).abs()
                < f64::EPSILON
        );
        assert_eq!(rebuilt.providers.ssh, original.providers.ssh);
        assert_eq!(
            rebuilt.appearance.colors.accent,
            original.appearance.colors.accent
        );
        assert_eq!(rebuilt.profiles.len(), original.profiles.len());
        assert_eq!(
            rebuilt.profiles["dev"].modes,
            original.profiles["dev"].modes
        );
    }

    // ── Legacy artifact detection ───────────────────────────────────────────

    fn write(path: &Path, contents: &str) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).expect("mkdir -p");
        }
        let mut f = std::fs::File::create(path).expect("create");
        f.write_all(contents.as_bytes()).expect("write");
    }

    #[test]
    fn scan_returns_empty_when_no_legacy_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let artifacts = scan_legacy_artifacts(dir.path());
        assert!(artifacts.is_empty());
    }

    #[test]
    fn scan_detects_plugin_with_full_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            &dir.path().join("plugins/hyprshutdown/plugin.toml"),
            r#"
[plugin]
id = "hyprshutdown"
name = "Hyprshutdown"
version = "0.1.0"
entry_point = "main.rn"

[[providers]]
id = "hyprshutdown"
prefix = ":hs"
icon = "system-shutdown"
"#,
        );
        write(&dir.path().join("plugins/hyprshutdown/main.rn"), "// rune");

        let artifacts = scan_legacy_artifacts(dir.path());
        assert_eq!(artifacts.len(), 1);
        match &artifacts[0].kind {
            LegacyKind::Plugin {
                id,
                name,
                prefix,
                icon,
                entry_point,
            } => {
                assert_eq!(id, "hyprshutdown");
                assert_eq!(name.as_deref(), Some("Hyprshutdown"));
                assert_eq!(prefix.as_deref(), Some(":hs"));
                assert_eq!(icon.as_deref(), Some("system-shutdown"));
                assert_eq!(entry_point.as_deref(), Some("main.rn"));
            }
            other => panic!("expected Plugin, got {:?}", other),
        }
    }

    #[test]
    fn scan_skips_directories_without_plugin_toml() {
        // A dir under plugins/ with no manifest is just user data
        // (editor configs, doc drafts, etc.) — not a real plugin.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("plugins/orphan")).expect("mkdir");
        std::fs::create_dir_all(dir.path().join("plugins/docs")).expect("mkdir");

        let artifacts = scan_legacy_artifacts(dir.path());
        assert!(
            artifacts.is_empty(),
            "dirs without plugin.toml must not be flagged; got: {:?}",
            artifacts
        );
    }

    #[test]
    fn scan_skips_dotfile_directories() {
        // .claude, .git, .cache etc. are never plugins.
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            &dir.path().join("plugins/.claude/plugin.toml"),
            "[plugin]\nid = \".claude\"\n",
        );
        let artifacts = scan_legacy_artifacts(dir.path());
        assert!(
            artifacts.is_empty(),
            "dotfile dirs must be skipped even with a manifest; got: {:?}",
            artifacts
        );
    }

    #[test]
    fn scan_tolerates_malformed_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            &dir.path().join("plugins/broken/plugin.toml"),
            "this is not = valid toml [[",
        );

        let artifacts = scan_legacy_artifacts(dir.path());
        assert_eq!(artifacts.len(), 1);
        match &artifacts[0].kind {
            LegacyKind::Plugin {
                id, name, prefix, ..
            } => {
                assert_eq!(id, "broken");
                assert!(name.is_none());
                assert!(prefix.is_none());
            }
            other => panic!("expected Plugin, got {:?}", other),
        }
    }

    #[test]
    fn scan_detects_scripts_directory_with_listing() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(&dir.path().join("scripts/foo.sh"), "#!/bin/sh\n");
        write(&dir.path().join("scripts/bar.sh"), "#!/bin/sh\n");

        let artifacts = scan_legacy_artifacts(dir.path());
        assert_eq!(artifacts.len(), 1);
        match &artifacts[0].kind {
            LegacyKind::ScriptsDir { entries } => {
                assert_eq!(entries.len(), 2);
                assert!(entries.contains(&"foo.sh".to_string()));
                assert!(entries.contains(&"bar.sh".to_string()));
            }
            other => panic!("expected ScriptsDir, got {:?}", other),
        }
    }

    #[test]
    fn scan_reports_both_plugins_and_scripts_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            &dir.path().join("plugins/p1/plugin.toml"),
            r#"[plugin] id = "p1""#,
        );
        write(&dir.path().join("scripts/x.sh"), "#!/bin/sh\n");

        let artifacts = scan_legacy_artifacts(dir.path());
        assert_eq!(artifacts.len(), 2);
        let kinds: Vec<&str> = artifacts
            .iter()
            .map(|a| match a.kind {
                LegacyKind::Plugin { .. } => "plugin",
                LegacyKind::ScriptsDir { .. } => "scripts",
            })
            .collect();
        assert!(kinds.contains(&"plugin"));
        assert!(kinds.contains(&"scripts"));
    }

    #[test]
    fn migrate_outcome_includes_legacy_artifacts_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let toml_path = write_toml(dir.path(), "[appearance]\ntheme = \"owl\"\n");
        let lua_path = dir.path().join("owlry.lua");
        write(
            &dir.path().join("plugins/hs/plugin.toml"),
            "[plugin]\nid = \"hs\"\n",
        );

        let req = MigrateRequest {
            toml_path: &toml_path,
            lua_path: &lua_path,
            force: false,
        };
        let outcome = migrate(&req).expect("migrate");
        assert_eq!(outcome.legacy_artifacts.len(), 1);
        match &outcome.legacy_artifacts[0].kind {
            LegacyKind::Plugin { id, .. } => assert_eq!(id, "hs"),
            other => panic!("expected Plugin, got {:?}", other),
        }
    }
}
