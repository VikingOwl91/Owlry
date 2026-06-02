//! `owlry.*` Lua API surface — table construction and function registration.
//!
//! Phase 3.2 installed three functions:
//!   - `owlry.set(table)`        — scalar overrides (last-write-wins per key)
//!   - `owlry.providers(list)`   — enabled provider list (replaces on call)
//!   - `owlry.tabs(list)`        — tab order list (replaces on call)
//!
//! Phase 3.3 added:
//!   - `owlry.provider(table)`   — register a user-defined provider
//!
//! Phase 3.5 adds:
//!   - `owlry.theme(name_or_table)` — theme name and/or inline colour overrides
//!   - `owlry.profiles(table)`      — named provider-id sets for `--profile`
//!
//! Phase 3.6 adds the `owlry.util.*` sub-table (shell, read_file, glob, env,
//! hostname). Built by [`super::util::build`] and attached here.
//!
//! Each closure shares an `Arc<Mutex<LuaConfig>>` with [`super::runtime`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::warn;
use mlua::{Function, Lua, Table, Value};

use super::config::{
    KNOWN_SET_KEYS, KNOWN_THEME_KEYS, LuaConfig, LuaProviderSpec, is_valid_provider_id,
};
use super::error::LuaConfigError;

/// Build the `owlry` table, register all functions on it, install it as a
/// global, and make `require("owlry")` return the same table.
pub(crate) fn install(lua: &Lua, state: Arc<Mutex<LuaConfig>>) -> Result<(), LuaConfigError> {
    let owlry = lua.create_table()?;

    register_set(lua, &owlry, Arc::clone(&state))?;
    register_providers(lua, &owlry, Arc::clone(&state))?;
    register_tabs(lua, &owlry, Arc::clone(&state))?;
    register_provider(lua, &owlry, Arc::clone(&state))?;
    register_theme(lua, &owlry, Arc::clone(&state))?;
    register_profiles(lua, &owlry, Arc::clone(&state))?;

    // owlry.util.* host helpers — stateless, no LuaConfig access needed.
    owlry.set("util", super::util::build(lua)?)?;

    lua.globals().set("owlry", owlry)?;

    // Make `require("owlry")` resolve to the global table. Done in Lua to
    // avoid the closure-Send dance around moving a Table into a function.
    lua.load(r#"package.preload["owlry"] = function() return owlry end"#)
        .exec()?;

    Ok(())
}

fn register_set(
    lua: &Lua,
    owlry: &Table,
    state: Arc<Mutex<LuaConfig>>,
) -> Result<(), LuaConfigError> {
    let f = lua.create_function(move |_, t: Table| {
        let mut cfg = state.lock().expect("lua config state mutex poisoned");
        apply_set(&mut cfg, t)
    })?;
    owlry.set("set", f)?;
    Ok(())
}

fn register_providers(
    lua: &Lua,
    owlry: &Table,
    state: Arc<Mutex<LuaConfig>>,
) -> Result<(), LuaConfigError> {
    let f = lua.create_function(move |_, t: Table| {
        let mut cfg = state.lock().expect("lua config state mutex poisoned");
        apply_providers(&mut cfg, t)
    })?;
    owlry.set("providers", f)?;
    Ok(())
}

fn register_tabs(
    lua: &Lua,
    owlry: &Table,
    state: Arc<Mutex<LuaConfig>>,
) -> Result<(), LuaConfigError> {
    let f = lua.create_function(move |_, t: Table| {
        let mut cfg = state.lock().expect("lua config state mutex poisoned");
        apply_tabs(&mut cfg, t)
    })?;
    owlry.set("tabs", f)?;
    Ok(())
}

fn register_provider(
    lua: &Lua,
    owlry: &Table,
    state: Arc<Mutex<LuaConfig>>,
) -> Result<(), LuaConfigError> {
    let f = lua.create_function(move |_, t: Table| {
        let mut cfg = state.lock().expect("lua config state mutex poisoned");
        apply_provider(&mut cfg, t)
    })?;
    owlry.set("provider", f)?;
    Ok(())
}

/// Merge keys from `owlry.set { ... }` into the accumulating config.
/// Per §4.1: multiple `set` calls merge — only keys present in this call
/// override existing values; unmentioned keys keep what they had.
fn apply_set(cfg: &mut LuaConfig, t: Table) -> mlua::Result<()> {
    if let Some(v) = t.get::<Option<String>>("theme")? {
        cfg.theme = Some(v);
    }
    if let Some(v) = t.get::<Option<i32>>("width")? {
        cfg.width = Some(v);
    }
    if let Some(v) = t.get::<Option<i32>>("height")? {
        cfg.height = Some(v);
    }
    if let Some(v) = t.get::<Option<u32>>("font_size")? {
        cfg.font_size = Some(v);
    }
    if let Some(v) = t.get::<Option<u32>>("border_radius")? {
        cfg.border_radius = Some(v);
    }
    if let Some(v) = t.get::<Option<String>>("terminal")? {
        cfg.terminal = Some(v);
    }
    if let Some(v) = t.get::<Option<bool>>("use_uwsm")? {
        cfg.use_uwsm = Some(v);
    }
    if let Some(v) = t.get::<Option<bool>>("show_icons")? {
        cfg.show_icons = Some(v);
    }
    if let Some(v) = t.get::<Option<usize>>("max_results")? {
        cfg.max_results = Some(v);
    }
    if let Some(v) = t.get::<Option<bool>>("frecency")? {
        cfg.frecency = Some(v);
    }
    if let Some(v) = t.get::<Option<f64>>("frecency_weight")? {
        cfg.frecency_weight = Some(v);
    }
    if let Some(v) = t.get::<Option<usize>>("frecency_max_entries")? {
        cfg.frecency_max_entries = Some(v);
    }
    if let Some(v) = t.get::<Option<i64>>("frecency_max_age_days")? {
        cfg.frecency_max_age_days = Some(v);
    }
    if let Some(v) = t.get::<Option<u32>>("frecency_min_launches")? {
        cfg.frecency_min_launches = Some(v);
    }
    if let Some(v) = t.get::<Option<String>>("search_engine")? {
        cfg.search_engine = Some(v);
    }
    if let Some(v) = t.get::<Option<Vec<String>>>("filesearch_roots")? {
        cfg.filesearch_roots = Some(v);
    }

    // Record unknown keys (string-keyed, non-known) for `config validate`.
    for pair in t.pairs::<mlua::Value, mlua::Value>() {
        let (k, _v) = pair?;
        if let mlua::Value::String(s) = k {
            let key = s.to_str()?.to_string();
            if !KNOWN_SET_KEYS.contains(&key.as_str()) && !cfg.unknown_settings.contains(&key) {
                cfg.unknown_settings.push(key);
            }
        }
    }

    Ok(())
}

/// `owlry.providers { ... }` — replaces the enabled list on each call (§4.2).
fn apply_providers(cfg: &mut LuaConfig, t: Table) -> mlua::Result<()> {
    let list: Vec<String> = t
        .sequence_values::<String>()
        .collect::<mlua::Result<Vec<_>>>()?;
    cfg.providers = Some(list);
    Ok(())
}

/// `owlry.tabs { ... }` — replaces the tab list on each call (§4.3).
fn apply_tabs(cfg: &mut LuaConfig, t: Table) -> mlua::Result<()> {
    let list: Vec<String> = t
        .sequence_values::<String>()
        .collect::<mlua::Result<Vec<_>>>()?;
    cfg.tabs = Some(list);
    Ok(())
}

/// `owlry.provider { id = ..., items = function() ... end, ... }` —
/// register a user-defined provider per §4.4. Accumulates across calls;
/// duplicate `id` replaces the prior registration with a warning.
fn apply_provider(cfg: &mut LuaConfig, t: Table) -> mlua::Result<()> {
    // Required: id and items. mlua converts missing keys → error here.
    let id: String = t.get("id").map_err(|_| {
        mlua::Error::RuntimeError(
            "owlry.provider: `id` is required (string, lowercase a-z 0-9 - _)".into(),
        )
    })?;

    if !is_valid_provider_id(&id) {
        return Err(mlua::Error::RuntimeError(format!(
            "owlry.provider: id '{}' invalid — must be lowercase alphanumeric with `-`/`_`",
            id
        )));
    }

    let items_fn: Function = t.get("items").map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "owlry.provider({}): `items` is required and must be a function",
            id
        ))
    })?;

    // Optional fields.
    let name: Option<String> = t.get("name")?;
    let prefix: Option<String> = t.get("prefix")?;
    let tab_label: Option<String> = t.get("tab_label")?;
    let icon: Option<String> = t.get("icon")?;
    let search_noun: Option<String> = t.get("search_noun")?;
    let priority: u32 = t.get::<Option<u32>>("priority")?.unwrap_or(0);
    let dynamic: bool = t.get::<Option<bool>>("dynamic")?.unwrap_or(false);

    if dynamic {
        return Err(mlua::Error::RuntimeError(format!(
            "owlry.provider({}): `dynamic = true` is not yet supported (lands in 2.2). \
             Drop the field or set it to false; 2.1 caches items once at startup.",
            id
        )));
    }

    let spec = LuaProviderSpec {
        id: id.clone(),
        name,
        prefix,
        tab_label,
        icon,
        search_noun,
        priority,
        dynamic,
        items_fn,
    };

    // Duplicate id → second wins, warning emitted (spec §6.4). Also recorded
    // on the snapshot for `owlry config validate` to surface.
    if let Some(pos) = cfg.user_providers.iter().position(|s| s.id == id) {
        warn!(
            "owlry.provider: duplicate id '{}' — second definition replaces the first",
            id
        );
        cfg.user_providers[pos] = spec;
        if !cfg.duplicate_user_provider_ids.contains(&id) {
            cfg.duplicate_user_provider_ids.push(id);
        }
    } else {
        cfg.user_providers.push(spec);
    }

    Ok(())
}

fn register_theme(
    lua: &Lua,
    owlry: &Table,
    state: Arc<Mutex<LuaConfig>>,
) -> Result<(), LuaConfigError> {
    let f = lua.create_function(move |_, v: Value| {
        let mut cfg = state.lock().expect("lua config state mutex poisoned");
        apply_theme(&mut cfg, v)
    })?;
    owlry.set("theme", f)?;
    Ok(())
}

fn register_profiles(
    lua: &Lua,
    owlry: &Table,
    state: Arc<Mutex<LuaConfig>>,
) -> Result<(), LuaConfigError> {
    let f = lua.create_function(move |_, t: Table| {
        let mut cfg = state.lock().expect("lua config state mutex poisoned");
        apply_profiles(&mut cfg, t)
    })?;
    owlry.set("profiles", f)?;
    Ok(())
}

/// `owlry.theme(...)` — accepts either a string (theme name) or a table
/// (inline colour overrides). Both forms can be combined across calls per
/// §4.5: name set by string-form, table-form layered on top.
fn apply_theme(cfg: &mut LuaConfig, value: Value) -> mlua::Result<()> {
    match value {
        Value::String(s) => {
            cfg.theme_name = Some(s.to_str()?.to_string());
            Ok(())
        }
        Value::Table(t) => apply_theme_table(cfg, t),
        other => Err(mlua::Error::RuntimeError(format!(
            "owlry.theme: expected a string (theme name) or table (colour overrides), got {}",
            other.type_name()
        ))),
    }
}

/// Read recognised colour keys from a theme table and overlay them on the
/// accumulated [`super::config::LuaConfig::theme_colors`]. Unknown keys are
/// recorded (forward-compat) — no error, mirroring `owlry.set` policy.
fn apply_theme_table(cfg: &mut LuaConfig, t: Table) -> mlua::Result<()> {
    macro_rules! read_colour {
        ($field:ident) => {
            if let Some(v) = t.get::<Option<String>>(stringify!($field))? {
                cfg.theme_colors.$field = Some(v);
            }
        };
    }
    read_colour!(background);
    read_colour!(background_secondary);
    read_colour!(border);
    read_colour!(text);
    read_colour!(text_secondary);
    read_colour!(accent);
    read_colour!(accent_bright);
    read_colour!(badge_app);
    read_colour!(badge_bookmark);
    read_colour!(badge_calc);
    read_colour!(badge_clip);
    read_colour!(badge_cmd);
    read_colour!(badge_dmenu);
    read_colour!(badge_emoji);
    read_colour!(badge_file);
    read_colour!(badge_script);
    read_colour!(badge_ssh);
    read_colour!(badge_power);
    read_colour!(badge_uuctl);
    read_colour!(badge_web);
    read_colour!(badge_media);
    read_colour!(badge_weather);
    read_colour!(badge_pomo);

    // Pre-v2 alias: badge_sys → badge_power (matches the serde alias on
    // ThemeColors in config/mod.rs). Only applies when the canonical key
    // wasn't already set in this call.
    if cfg.theme_colors.badge_power.is_none()
        && let Some(v) = t.get::<Option<String>>("badge_sys")?
    {
        cfg.theme_colors.badge_power = Some(v);
    }

    // Track unknown theme keys for `owlry config validate`.
    for pair in t.pairs::<Value, Value>() {
        let (k, _) = pair?;
        if let Value::String(s) = k {
            let key = s.to_str()?.to_string();
            if !KNOWN_THEME_KEYS.contains(&key.as_str()) && !cfg.unknown_theme_keys.contains(&key) {
                cfg.unknown_theme_keys.push(key);
            }
        }
    }

    Ok(())
}

/// `owlry.profiles { dev = { "app", "cmd" }, ... }` — captures named
/// provider-id sets used by `--profile <name>`. Replaces the entire map
/// on each call (consistent with providers/tabs replace-on-call semantics).
fn apply_profiles(cfg: &mut LuaConfig, t: Table) -> mlua::Result<()> {
    let mut profiles: HashMap<String, Vec<String>> = HashMap::new();
    for pair in t.pairs::<String, Table>() {
        let (name, modes_tbl) = pair.map_err(|e| {
            mlua::Error::RuntimeError(format!(
                "owlry.profiles: every value must be a list of provider ids — {}",
                e
            ))
        })?;
        let modes: Vec<String> = modes_tbl
            .sequence_values::<String>()
            .collect::<mlua::Result<Vec<_>>>()
            .map_err(|e| {
                mlua::Error::RuntimeError(format!(
                    "owlry.profiles.{}: list entries must be strings — {}",
                    name, e
                ))
            })?;
        profiles.insert(name, modes);
    }
    cfg.profiles = Some(profiles);
    Ok(())
}
