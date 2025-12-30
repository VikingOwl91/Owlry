//! Hook API for Lua plugins
//!
//! Allows plugins to register callbacks for application events:
//! - `owlry.hook.on(event, callback)` - Register a hook
//! - Events: init, query, results, select, pre_launch, post_launch, shutdown

use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Hook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    /// Called when plugin is initialized
    Init,
    /// Called when query changes, can modify query
    Query,
    /// Called after results are gathered, can filter/modify results
    Results,
    /// Called when an item is selected (highlighted)
    Select,
    /// Called before launching an item, can cancel launch
    PreLaunch,
    /// Called after launching an item
    PostLaunch,
    /// Called when application is shutting down
    Shutdown,
}

impl HookEvent {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "init" => Some(Self::Init),
            "query" => Some(Self::Query),
            "results" => Some(Self::Results),
            "select" => Some(Self::Select),
            "pre_launch" | "prelaunch" => Some(Self::PreLaunch),
            "post_launch" | "postlaunch" => Some(Self::PostLaunch),
            "shutdown" => Some(Self::Shutdown),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Query => "query",
            Self::Results => "results",
            Self::Select => "select",
            Self::PreLaunch => "pre_launch",
            Self::PostLaunch => "post_launch",
            Self::Shutdown => "shutdown",
        }
    }
}

/// Registered hook information
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used for hook inspection
pub struct HookRegistration {
    pub event: HookEvent,
    pub plugin_id: String,
    pub priority: i32,
}

/// Type alias for hook handlers: (plugin_id, priority)
type HookHandlers = Vec<(String, i32)>;

/// Global hook registry
/// Maps event -> list of (plugin_id, priority)
static HOOK_REGISTRY: LazyLock<Mutex<HashMap<HookEvent, HookHandlers>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register hook APIs
pub fn register_hook_api(lua: &Lua, owlry: &Table, plugin_id: &str) -> LuaResult<()> {
    let hook_table = lua.create_table()?;
    let plugin_id_owned = plugin_id.to_string();

    // Store plugin_id in registry for later use
    lua.set_named_registry_value("plugin_id", plugin_id_owned.clone())?;

    // Initialize hook storage in Lua registry
    if lua.named_registry_value::<Value>("hooks")?.is_nil() {
        let hooks: Table = lua.create_table()?;
        lua.set_named_registry_value("hooks", hooks)?;
    }

    // owlry.hook.on(event, callback, priority?) -> boolean
    // Register a hook for an event
    let plugin_id_for_closure = plugin_id_owned.clone();
    hook_table.set(
        "on",
        lua.create_function(move |lua, (event_name, callback, priority): (String, Function, Option<i32>)| {
            let event = HookEvent::from_str(&event_name).ok_or_else(|| {
                mlua::Error::external(format!(
                    "Unknown hook event '{}'. Valid events: init, query, results, select, pre_launch, post_launch, shutdown",
                    event_name
                ))
            })?;

            let priority = priority.unwrap_or(0);

            // Store callback in Lua registry
            let hooks: Table = lua.named_registry_value("hooks")?;
            let event_key = event.as_str();

            let event_hooks: Table = if let Ok(t) = hooks.get::<Table>(event_key) {
                t
            } else {
                let t = lua.create_table()?;
                hooks.set(event_key, t.clone())?;
                t
            };

            // Add callback to event hooks
            let len = event_hooks.len()? + 1;
            let hook_entry = lua.create_table()?;
            hook_entry.set("callback", callback)?;
            hook_entry.set("priority", priority)?;
            event_hooks.set(len, hook_entry)?;

            // Register in global registry
            let mut registry = HOOK_REGISTRY.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock hook registry: {}", e))
            })?;

            let hooks_list = registry.entry(event).or_insert_with(Vec::new);
            hooks_list.push((plugin_id_for_closure.clone(), priority));
            // Sort by priority (higher priority first)
            hooks_list.sort_by(|a, b| b.1.cmp(&a.1));

            log::debug!(
                "[plugin:{}] Registered hook for '{}' with priority {}",
                plugin_id_for_closure,
                event_name,
                priority
            );

            Ok(true)
        })?,
    )?;

    // owlry.hook.off(event) -> boolean
    // Unregister all hooks for an event from this plugin
    let plugin_id_for_off = plugin_id_owned.clone();
    hook_table.set(
        "off",
        lua.create_function(move |lua, event_name: String| {
            let event = HookEvent::from_str(&event_name).ok_or_else(|| {
                mlua::Error::external(format!("Unknown hook event '{}'", event_name))
            })?;

            // Remove from Lua registry
            let hooks: Table = lua.named_registry_value("hooks")?;
            hooks.set(event.as_str(), Value::Nil)?;

            // Remove from global registry
            let mut registry = HOOK_REGISTRY.lock().map_err(|e| {
                mlua::Error::external(format!("Failed to lock hook registry: {}", e))
            })?;

            if let Some(hooks_list) = registry.get_mut(&event) {
                hooks_list.retain(|(id, _)| id != &plugin_id_for_off);
            }

            log::debug!(
                "[plugin:{}] Unregistered hooks for '{}'",
                plugin_id_for_off,
                event_name
            );

            Ok(true)
        })?,
    )?;

    owlry.set("hook", hook_table)?;
    Ok(())
}

/// Call hooks for a specific event in a Lua runtime
/// Returns the (possibly modified) value
#[allow(dead_code)] // Will be used by UI integration
pub fn call_hooks<T>(lua: &Lua, event: HookEvent, value: T) -> LuaResult<T>
where
    T: mlua::IntoLua + mlua::FromLua,
{
    let hooks: Table = match lua.named_registry_value("hooks") {
        Ok(h) => h,
        Err(_) => return Ok(value), // No hooks registered
    };

    let event_hooks: Table = match hooks.get(event.as_str()) {
        Ok(h) => h,
        Err(_) => return Ok(value), // No hooks for this event
    };

    let mut current_value = value.into_lua(lua)?;

    // Collect hooks with priorities
    let mut hook_entries: Vec<(i32, Function)> = Vec::new();
    for pair in event_hooks.pairs::<i64, Table>() {
        let (_, entry) = pair?;
        let priority: i32 = entry.get("priority").unwrap_or(0);
        let callback: Function = entry.get("callback")?;
        hook_entries.push((priority, callback));
    }

    // Sort by priority (higher first)
    hook_entries.sort_by(|a, b| b.0.cmp(&a.0));

    // Call each hook
    for (_, callback) in hook_entries {
        match callback.call::<Value>(current_value.clone()) {
            Ok(result) => {
                // If hook returns non-nil, use it as the new value
                if !result.is_nil() {
                    current_value = result;
                }
            }
            Err(e) => {
                log::warn!("[hook:{}] Hook callback failed: {}", event.as_str(), e);
                // Continue with other hooks
            }
        }
    }

    T::from_lua(current_value, lua)
}

/// Call hooks that return a boolean (for pre_launch cancellation)
#[allow(dead_code)] // Will be used for pre_launch hooks
pub fn call_hooks_bool(lua: &Lua, event: HookEvent, value: Value) -> LuaResult<bool> {
    let hooks: Table = match lua.named_registry_value("hooks") {
        Ok(h) => h,
        Err(_) => return Ok(true), // No hooks, allow
    };

    let event_hooks: Table = match hooks.get(event.as_str()) {
        Ok(h) => h,
        Err(_) => return Ok(true), // No hooks for this event
    };

    // Collect and sort hooks
    let mut hook_entries: Vec<(i32, Function)> = Vec::new();
    for pair in event_hooks.pairs::<i64, Table>() {
        let (_, entry) = pair?;
        let priority: i32 = entry.get("priority").unwrap_or(0);
        let callback: Function = entry.get("callback")?;
        hook_entries.push((priority, callback));
    }
    hook_entries.sort_by(|a, b| b.0.cmp(&a.0));

    // Call each hook - if any returns false, cancel
    for (_, callback) in hook_entries {
        match callback.call::<Value>(value.clone()) {
            Ok(result) => {
                if let Value::Boolean(false) = result {
                    return Ok(false); // Cancel
                }
            }
            Err(e) => {
                log::warn!("[hook:{}] Hook callback failed: {}", event.as_str(), e);
            }
        }
    }

    Ok(true)
}

/// Call hooks with no return value (for notifications)
#[allow(dead_code)] // Will be used for notification hooks
pub fn call_hooks_void(lua: &Lua, event: HookEvent, value: Value) -> LuaResult<()> {
    let hooks: Table = match lua.named_registry_value("hooks") {
        Ok(h) => h,
        Err(_) => return Ok(()), // No hooks
    };

    let event_hooks: Table = match hooks.get(event.as_str()) {
        Ok(h) => h,
        Err(_) => return Ok(()), // No hooks for this event
    };

    for pair in event_hooks.pairs::<i64, Table>() {
        let (_, entry) = pair?;
        let callback: Function = entry.get("callback")?;
        if let Err(e) = callback.call::<()>(value.clone()) {
            log::warn!("[hook:{}] Hook callback failed: {}", event.as_str(), e);
        }
    }

    Ok(())
}

/// Get list of plugins that have registered for an event
#[allow(dead_code)]
pub fn get_registered_plugins(event: HookEvent) -> Vec<String> {
    HOOK_REGISTRY
        .lock()
        .map(|r| {
            r.get(&event)
                .map(|v| v.iter().map(|(id, _)| id.clone()).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Clear all hooks (used when reloading plugins)
#[allow(dead_code)]
pub fn clear_all_hooks() {
    if let Ok(mut registry) = HOOK_REGISTRY.lock() {
        registry.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua(plugin_id: &str) -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_hook_api(&lua, &owlry, plugin_id).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_hook_registration() {
        clear_all_hooks();
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(r#"
            local called = false
            owlry.hook.on("init", function()
                called = true
            end)
            return true
        "#);
        let result: bool = chunk.call(()).unwrap();
        assert!(result);

        // Verify hook was registered
        let plugins = get_registered_plugins(HookEvent::Init);
        assert!(plugins.contains(&"test-plugin".to_string()));
    }

    #[test]
    fn test_hook_with_priority() {
        clear_all_hooks();
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(r#"
            owlry.hook.on("query", function(q) return q .. "1" end, 10)
            owlry.hook.on("query", function(q) return q .. "2" end, 20)
            return true
        "#);
        chunk.call::<()>(()).unwrap();

        // Call hooks - higher priority (20) should run first
        let result: String = call_hooks(&lua, HookEvent::Query, "test".to_string()).unwrap();
        // Priority 20 adds "2" first, then priority 10 adds "1"
        assert_eq!(result, "test21");
    }

    #[test]
    fn test_hook_off() {
        clear_all_hooks();
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(r#"
            owlry.hook.on("select", function() end)
            owlry.hook.off("select")
            return true
        "#);
        chunk.call::<()>(()).unwrap();

        let plugins = get_registered_plugins(HookEvent::Select);
        assert!(!plugins.contains(&"test-plugin".to_string()));
    }

    #[test]
    fn test_pre_launch_cancel() {
        clear_all_hooks();
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(r#"
            owlry.hook.on("pre_launch", function(item)
                if item.name == "blocked" then
                    return false -- cancel launch
                end
                return true
            end)
        "#);
        chunk.call::<()>(()).unwrap();

        // Create a test item table
        let item = lua.create_table().unwrap();
        item.set("name", "blocked").unwrap();

        let allow = call_hooks_bool(&lua, HookEvent::PreLaunch, Value::Table(item)).unwrap();
        assert!(!allow); // Should be blocked

        // Test with allowed item
        let item2 = lua.create_table().unwrap();
        item2.set("name", "allowed").unwrap();

        let allow2 = call_hooks_bool(&lua, HookEvent::PreLaunch, Value::Table(item2)).unwrap();
        assert!(allow2); // Should be allowed
    }
}
