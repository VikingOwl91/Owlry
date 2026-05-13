//! `owlry.*` Lua API surface — table construction and function registration.
//!
//! Phase 3.2 installed three functions:
//!   - `owlry.set(table)`        — scalar overrides (last-write-wins per key)
//!   - `owlry.providers(list)`   — enabled provider list (replaces on call)
//!   - `owlry.tabs(list)`        — tab order list (replaces on call)
//!
//! Phase 3.3 adds:
//!   - `owlry.provider(table)`   — register a user-defined provider
//!
//! Each closure shares an `Arc<Mutex<LuaConfig>>` with [`super::runtime`].
//! Later sub-phases add `owlry.theme` (3.5) and `owlry.util.*` (3.6).

use std::sync::{Arc, Mutex};

use log::warn;
use mlua::{Function, Lua, Table};

use super::config::{KNOWN_SET_KEYS, LuaConfig, LuaProviderSpec, is_valid_provider_id};
use super::error::LuaConfigError;

/// Build the `owlry` table, register all functions on it, install it as a
/// global, and make `require("owlry")` return the same table.
pub(crate) fn install(lua: &Lua, state: Arc<Mutex<LuaConfig>>) -> Result<(), LuaConfigError> {
    let owlry = lua.create_table()?;

    register_set(lua, &owlry, Arc::clone(&state))?;
    register_providers(lua, &owlry, Arc::clone(&state))?;
    register_tabs(lua, &owlry, Arc::clone(&state))?;
    register_provider(lua, &owlry, Arc::clone(&state))?;

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
    if let Some(v) = t.get::<Option<String>>("search_engine")? {
        cfg.search_engine = Some(v);
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

    // Duplicate id → second wins, warning emitted (spec §6.4).
    if let Some(pos) = cfg.user_providers.iter().position(|s| s.id == id) {
        warn!(
            "owlry.provider: duplicate id '{}' — second definition replaces the first",
            id
        );
        cfg.user_providers[pos] = spec;
    } else {
        cfg.user_providers.push(spec);
    }

    Ok(())
}
