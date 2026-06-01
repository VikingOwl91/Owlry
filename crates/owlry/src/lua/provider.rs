//! [`LuaProvider`] — bridges a [`LuaProviderSpec`] (a captured `owlry.provider {}`
//! registration) onto the host-side [`Provider`] trait.
//!
//! Phase 3.3 ships **static** providers only (per `docs/lua-api.md` §4.4,
//! "2.1 ships only `dynamic = false`"): the Lua `items` function is called
//! once via [`Provider::refresh`] with an empty query, results are cached,
//! and subsequent `Provider::items` calls return that cache. `dynamic = true`
//! lands in Phase 3 follow-up (2.2).
//!
//! Errors raised by the user's `items` function are logged and result in an
//! empty cache for that refresh — the daemon must never crash because of a
//! broken user provider.

use std::sync::Arc;

use log::{debug, error, warn};
use mlua::{Lua, Table};

use crate::providers::{ItemSource, LaunchItem, Provider, ProviderType};

use super::config::LuaProviderSpec;

const DEFAULT_ICON: &str = "application-x-addon";

pub struct LuaProvider {
    spec: LuaProviderSpec,
    /// Keeps the parent Lua state alive — `mlua::Function` references don't
    /// bump the Lua refcount on their own, so dropping the state would
    /// invalidate `spec.items_fn`. The handle is never accessed directly.
    _lua: Arc<Lua>,
    cached: Vec<LaunchItem>,
}

impl LuaProvider {
    pub fn new(spec: LuaProviderSpec, lua: Arc<Lua>) -> Self {
        Self {
            spec,
            _lua: lua,
            cached: Vec::new(),
        }
    }

    /// The user-supplied id (also the [`ProviderType::Plugin`] payload).
    pub fn id(&self) -> &str {
        &self.spec.id
    }

    fn type_id(&self) -> String {
        self.spec.id.clone()
    }
}

impl Provider for LuaProvider {
    fn name(&self) -> &str {
        self.spec.name.as_deref().unwrap_or(self.spec.id.as_str())
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(self.type_id())
    }

    fn priority(&self) -> u32 {
        self.spec.priority
    }

    fn prefix(&self) -> Option<&str> {
        self.spec.prefix.as_deref()
    }

    fn icon(&self) -> &str {
        self.spec.icon.as_deref().unwrap_or(DEFAULT_ICON)
    }

    fn tab_label(&self) -> Option<&str> {
        self.spec.tab_label.as_deref()
    }

    fn search_noun(&self) -> Option<&str> {
        self.spec.search_noun.as_deref()
    }

    fn refresh(&mut self) {
        // Spec §4.4: dynamic=false is "called once at startup and cached".
        // We pass an empty query — the user's items function is expected to
        // return the full universe when no query is supplied.
        match self.spec.items_fn.call::<Vec<Table>>("") {
            Ok(rows) => {
                let mut items = Vec::with_capacity(rows.len());
                for (idx, row) in rows.into_iter().enumerate() {
                    match parse_item(&row, &self.spec.id, idx) {
                        Ok(it) => items.push(it),
                        Err(e) => warn!(
                            "user provider '{}': dropping item #{} — {}",
                            self.spec.id, idx, e
                        ),
                    }
                }
                debug!(
                    "user provider '{}': refreshed {} item(s)",
                    self.spec.id,
                    items.len()
                );
                self.cached = items;
            }
            Err(e) => {
                error!(
                    "user provider '{}': items() raised — {} — returning 0 results",
                    self.spec.id, e
                );
                self.cached.clear();
            }
        }
    }

    fn items(&self) -> &[LaunchItem] {
        &self.cached
    }
}

/// Parse a single Lua item table into a [`LaunchItem`]. `idx` is the
/// 0-based position in the returned list — used to synthesize a stable item id.
fn parse_item(t: &Table, provider_id: &str, idx: usize) -> mlua::Result<LaunchItem> {
    let name: String = t.get("name").map_err(|_| {
        mlua::Error::RuntimeError("item missing required field `name` (string)".into())
    })?;
    let command: String = t.get("command").map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "item '{}' missing required field `command` (string)",
            name
        ))
    })?;
    let description: Option<String> = t.get("description")?;
    let icon: Option<String> = t.get("icon")?;
    let terminal: bool = t.get::<Option<bool>>("terminal")?.unwrap_or(false);
    let tags: Vec<String> = t
        .get::<Option<Table>>("tags")?
        .map(|tt| {
            tt.sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(LaunchItem {
        id: format!("{}:{}:{}", provider_id, idx, name),
        name,
        description,
        icon,
        provider: ProviderType::Plugin(provider_id.to_string()),
        command,
        terminal,
        tags,
        source: ItemSource::ScriptPlugin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua::runtime::LuaContext;

    /// Build a LuaProvider from a single `owlry.provider {}` invocation.
    /// The LuaContext is dropped at the end of `build` — the `Arc<Lua>` held
    /// by the returned LuaProvider keeps the underlying state alive.
    fn build(src: &str) -> LuaProvider {
        let ctx = LuaContext::new().expect("ctx");
        ctx.eval_str(src).expect("script");
        let lua = ctx.lua_handle();
        let cfg = ctx.snapshot();
        let spec = cfg
            .user_providers
            .into_iter()
            .next()
            .expect("expected one user provider");
        LuaProvider::new(spec, lua)
    }

    #[test]
    fn minimal_provider_captures_id_and_defaults() {
        let p = build(r#"owlry.provider { id = "hs", items = function() return {} end }"#);
        assert_eq!(p.id(), "hs");
        assert_eq!(p.name(), "hs"); // name defaults to id
        assert_eq!(p.icon(), "application-x-addon");
        assert!(p.prefix().is_none());
        assert!(p.tab_label().is_none());
        assert!(p.search_noun().is_none());
        assert_eq!(p.priority(), 0);
        assert_eq!(p.provider_type(), ProviderType::Plugin("hs".into()));
    }

    #[test]
    fn full_provider_captures_all_optional_fields() {
        let p = build(
            r#"
            owlry.provider {
              id          = "hs",
              name        = "Hyprland Shutdown",
              prefix      = ":hs",
              tab_label   = "Shutdown",
              icon        = "system-shutdown",
              search_noun = "shutdown actions",
              priority    = 42,
              items       = function() return {} end,
            }
            "#,
        );
        assert_eq!(p.id(), "hs");
        assert_eq!(p.name(), "Hyprland Shutdown");
        assert_eq!(p.prefix(), Some(":hs"));
        assert_eq!(p.tab_label(), Some("Shutdown"));
        assert_eq!(p.icon(), "system-shutdown");
        assert_eq!(p.search_noun(), Some("shutdown actions"));
        assert_eq!(p.priority(), 42);
    }

    #[test]
    fn refresh_calls_items_function_and_caches() {
        let mut p = build(
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
        assert!(p.items().is_empty(), "must start empty before refresh");
        p.refresh();
        let items = p.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Lock");
        assert_eq!(items[0].command, "hyprlock");
        assert_eq!(items[0].provider, ProviderType::Plugin("hs".into()));
        assert_eq!(items[0].source, ItemSource::ScriptPlugin);
        assert!(!items[0].terminal);
        assert!(items[0].tags.is_empty());
        assert!(items[0].description.is_none());

        assert_eq!(items[1].name, "Shutdown");
        assert_eq!(items[1].command, "systemctl poweroff");
    }

    #[test]
    fn refresh_parses_all_optional_item_fields() {
        let mut p = build(
            r#"
            owlry.provider {
              id = "hs",
              items = function()
                return {
                  {
                    name = "Reboot",
                    command = "systemctl reboot",
                    description = "Reboot the machine",
                    icon = "system-reboot",
                    terminal = true,
                    tags = { "power", "destructive" },
                  },
                }
              end,
            }
            "#,
        );
        p.refresh();
        let it = &p.items()[0];
        assert_eq!(it.name, "Reboot");
        assert_eq!(it.command, "systemctl reboot");
        assert_eq!(it.description.as_deref(), Some("Reboot the machine"));
        assert_eq!(it.icon.as_deref(), Some("system-reboot"));
        assert!(it.terminal);
        assert_eq!(it.tags, vec!["power", "destructive"]);
    }

    #[test]
    fn refresh_drops_items_missing_required_fields() {
        let mut p = build(
            r#"
            owlry.provider {
              id = "hs",
              items = function()
                return {
                  { name = "Lock", command = "hyprlock" },
                  { name = "No command here" },         -- missing command, drop
                  { command = "no name here" },         -- missing name, drop
                  { name = "Reboot", command = "systemctl reboot" },
                }
              end,
            }
            "#,
        );
        p.refresh();
        let items = p.items();
        assert_eq!(items.len(), 2, "only well-formed items kept");
        assert_eq!(items[0].name, "Lock");
        assert_eq!(items[1].name, "Reboot");
    }

    #[test]
    fn refresh_handles_lua_runtime_error_gracefully() {
        let mut p = build(
            r#"
            owlry.provider {
              id = "broken",
              items = function() error("kaboom") end,
            }
            "#,
        );
        // Should not panic; cache remains empty.
        p.refresh();
        assert!(p.items().is_empty());
    }

    #[test]
    fn refresh_handles_non_table_return_gracefully() {
        let mut p = build(
            r#"
            owlry.provider {
              id = "weird",
              items = function() return "not a table" end,
            }
            "#,
        );
        p.refresh();
        // Cache empty — the mlua::Vec<Table> conversion fails, treated as error.
        assert!(p.items().is_empty());
    }

    #[test]
    fn lua_provider_is_send_and_sync() {
        // Compile-time check: needed because ProviderManager stores providers
        // as `Box<dyn Provider>` where `Provider: Send + Sync`.
        fn assert<T: Send + Sync>() {}
        assert::<LuaProvider>();
    }

    #[test]
    fn item_ids_are_unique_per_provider() {
        let mut p = build(
            r#"
            owlry.provider {
              id = "hs",
              items = function()
                return {
                  { name = "Same", command = "a" },
                  { name = "Same", command = "b" },
                }
              end,
            }
            "#,
        );
        p.refresh();
        let items = p.items();
        assert_ne!(
            items[0].id, items[1].id,
            "different positions must produce different ids"
        );
    }
}

#[cfg(test)]
mod registration_tests {
    //! Tests for the `owlry.provider` registration path itself (validation,
    //! accumulation, duplicate handling). These live here rather than in
    //! `runtime.rs` because they exercise [`super::super::config::LuaProviderSpec`].

    use crate::lua::runtime::LuaContext;

    fn run(src: &str) -> Result<crate::lua::LuaConfig, crate::lua::LuaConfigError> {
        let ctx = LuaContext::new()?;
        ctx.eval_str(src)?;
        Ok(ctx.snapshot())
    }

    #[test]
    fn accumulates_multiple_distinct_providers() {
        let cfg = run(r#"
            owlry.provider { id = "a", items = function() return {} end }
            owlry.provider { id = "b", items = function() return {} end }
            owlry.provider { id = "c", items = function() return {} end }
            "#)
        .unwrap();
        let ids: Vec<&str> = cfg.user_providers.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn duplicate_id_replaces_in_place() {
        let cfg = run(r#"
            owlry.provider { id = "a", prefix = ":v1", items = function() return {} end }
            owlry.provider { id = "b", items = function() return {} end }
            owlry.provider { id = "a", prefix = ":v2", items = function() return {} end }
            "#)
        .unwrap();
        assert_eq!(cfg.user_providers.len(), 2);
        let a = cfg.user_providers.iter().find(|s| s.id == "a").unwrap();
        assert_eq!(a.prefix.as_deref(), Some(":v2"), "second `a` wins");
    }

    #[test]
    fn missing_id_is_an_error() {
        let err = run(r#"owlry.provider { items = function() return {} end }"#).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("`id` is required"), "got: {}", msg);
    }

    #[test]
    fn missing_items_is_an_error() {
        let err = run(r#"owlry.provider { id = "hs" }"#).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("`items` is required"), "got: {}", msg);
    }

    #[test]
    fn invalid_id_format_is_an_error() {
        let err = run(r#"owlry.provider { id = "BAD ID", items = function() return {} end }"#)
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("invalid"), "got: {}", msg);
    }

    #[test]
    fn dynamic_true_is_rejected_in_2_1() {
        let err = run(r#"
            owlry.provider {
              id = "x", dynamic = true,
              items = function() return {} end,
            }
            "#)
        .unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("dynamic"), "got: {}", msg);
        assert!(
            msg.contains("2.2"),
            "should mention 2.2 roadmap; got: {}",
            msg
        );
    }

    #[test]
    fn dynamic_false_explicit_is_fine() {
        let cfg = run(r#"
            owlry.provider {
              id = "x", dynamic = false,
              items = function() return {} end,
            }
            "#)
        .unwrap();
        assert_eq!(cfg.user_providers.len(), 1);
        assert!(!cfg.user_providers[0].dynamic);
    }

    #[test]
    fn priority_is_captured() {
        let cfg = run(r#"
            owlry.provider {
              id = "x", priority = 999,
              items = function() return {} end,
            }
            "#)
        .unwrap();
        assert_eq!(cfg.user_providers[0].priority, 999);
    }
}
