//! Action API for Lua plugins
//!
//! Allows plugins to register custom actions for result items:
//! - `owlry.action.register(config)` - Register a custom action

use mlua::{Function, Lua, Result as LuaResult, Table, Value};

/// Action registration data
#[derive(Debug, Clone)]
#[allow(dead_code)] // Used by UI integration
pub struct ActionRegistration {
    /// Unique action ID
    pub id: String,
    /// Human-readable name shown in UI
    pub display_name: String,
    /// Icon name (optional)
    pub icon: Option<String>,
    /// Keyboard shortcut hint (optional, e.g., "Ctrl+C")
    pub shortcut: Option<String>,
    /// Plugin that registered this action
    pub plugin_id: String,
}

/// Register action APIs
pub fn register_action_api(lua: &Lua, owlry: &Table, plugin_id: &str) -> LuaResult<()> {
    let action_table = lua.create_table()?;
    let plugin_id_owned = plugin_id.to_string();

    // Initialize action storage in Lua registry
    if lua.named_registry_value::<Value>("actions")?.is_nil() {
        let actions: Table = lua.create_table()?;
        lua.set_named_registry_value("actions", actions)?;
    }

    // owlry.action.register(config) -> string (action_id)
    // config = {
    //   id = "copy-url",
    //   name = "Copy URL",
    //   icon = "edit-copy",  -- optional
    //   shortcut = "Ctrl+C", -- optional
    //   filter = function(item) return item.provider == "bookmarks" end,  -- optional
    //   handler = function(item) ... end
    // }
    let plugin_id_for_register = plugin_id_owned.clone();
    action_table.set(
        "register",
        lua.create_function(move |lua, config: Table| {
            // Extract required fields
            let id: String = config
                .get("id")
                .map_err(|_| mlua::Error::external("action.register: 'id' is required"))?;

            let name: String = config
                .get("name")
                .map_err(|_| mlua::Error::external("action.register: 'name' is required"))?;

            let _handler: Function = config.get("handler").map_err(|_| {
                mlua::Error::external("action.register: 'handler' function is required")
            })?;

            // Extract optional fields
            let icon: Option<String> = config.get("icon").ok();
            let shortcut: Option<String> = config.get("shortcut").ok();

            // Store action in registry
            let actions: Table = lua.named_registry_value("actions")?;

            // Create full action ID with plugin prefix
            let full_id = format!("{}:{}", plugin_id_for_register, id);

            // Store config with full ID
            let action_entry = lua.create_table()?;
            action_entry.set("id", full_id.clone())?;
            action_entry.set("name", name.clone())?;
            action_entry.set("plugin_id", plugin_id_for_register.clone())?;
            if let Some(ref i) = icon {
                action_entry.set("icon", i.clone())?;
            }
            if let Some(ref s) = shortcut {
                action_entry.set("shortcut", s.clone())?;
            }
            // Store filter and handler functions
            if let Ok(filter) = config.get::<Function>("filter") {
                action_entry.set("filter", filter)?;
            }
            action_entry.set("handler", config.get::<Function>("handler")?)?;

            actions.set(full_id.clone(), action_entry)?;

            log::info!(
                "[plugin:{}] Registered action '{}' ({})",
                plugin_id_for_register,
                name,
                full_id
            );

            Ok(full_id)
        })?,
    )?;

    // owlry.action.unregister(id) -> boolean
    let plugin_id_for_unregister = plugin_id_owned.clone();
    action_table.set(
        "unregister",
        lua.create_function(move |lua, id: String| {
            let actions: Table = lua.named_registry_value("actions")?;
            let full_id = format!("{}:{}", plugin_id_for_unregister, id);

            if actions.contains_key(full_id.clone())? {
                actions.set(full_id, Value::Nil)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?,
    )?;

    owlry.set("action", action_table)?;
    Ok(())
}

/// Get all registered actions from a Lua runtime
#[allow(dead_code)] // Will be used by UI
pub fn get_actions(lua: &Lua) -> LuaResult<Vec<ActionRegistration>> {
    let actions: Table = match lua.named_registry_value("actions") {
        Ok(a) => a,
        Err(_) => return Ok(Vec::new()),
    };

    let mut result = Vec::new();

    for pair in actions.pairs::<String, Table>() {
        let (_, entry) = pair?;

        let id: String = entry.get("id")?;
        let display_name: String = entry.get("name")?;
        let plugin_id: String = entry.get("plugin_id")?;
        let icon: Option<String> = entry.get("icon").ok();
        let shortcut: Option<String> = entry.get("shortcut").ok();

        result.push(ActionRegistration {
            id,
            display_name,
            icon,
            shortcut,
            plugin_id,
        });
    }

    Ok(result)
}

/// Get actions that apply to a specific item
#[allow(dead_code)] // Will be used by UI context menu
pub fn get_actions_for_item(lua: &Lua, item: &Table) -> LuaResult<Vec<ActionRegistration>> {
    let actions: Table = match lua.named_registry_value("actions") {
        Ok(a) => a,
        Err(_) => return Ok(Vec::new()),
    };

    let mut result = Vec::new();

    for pair in actions.pairs::<String, Table>() {
        let (_, entry) = pair?;

        // Check filter if present
        if let Ok(filter) = entry.get::<Function>("filter") {
            match filter.call::<bool>(item.clone()) {
                Ok(true) => {}         // Include this action
                Ok(false) => continue, // Skip this action
                Err(e) => {
                    log::warn!("Action filter failed: {}", e);
                    continue;
                }
            }
        }

        let id: String = entry.get("id")?;
        let display_name: String = entry.get("name")?;
        let plugin_id: String = entry.get("plugin_id")?;
        let icon: Option<String> = entry.get("icon").ok();
        let shortcut: Option<String> = entry.get("shortcut").ok();

        result.push(ActionRegistration {
            id,
            display_name,
            icon,
            shortcut,
            plugin_id,
        });
    }

    Ok(result)
}

/// Execute an action by ID
#[allow(dead_code)] // Will be used by UI
pub fn execute_action(lua: &Lua, action_id: &str, item: &Table) -> LuaResult<()> {
    let actions: Table = lua.named_registry_value("actions")?;
    let action: Table = actions.get(action_id)?;
    let handler: Function = action.get("handler")?;

    handler.call::<()>(item.clone())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua(plugin_id: &str) -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_action_api(&lua, &owlry, plugin_id).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_action_registration() {
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(
            r#"
            return owlry.action.register({
                id = "copy-name",
                name = "Copy Name",
                icon = "edit-copy",
                handler = function(item)
                    -- copy logic here
                end
            })
        "#,
        );
        let action_id: String = chunk.call(()).unwrap();
        assert_eq!(action_id, "test-plugin:copy-name");

        // Verify action is registered
        let actions = get_actions(&lua).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].display_name, "Copy Name");
    }

    #[test]
    fn test_action_with_filter() {
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(
            r#"
            owlry.action.register({
                id = "bookmark-action",
                name = "Open in Browser",
                filter = function(item)
                    return item.provider == "bookmarks"
                end,
                handler = function(item) end
            })
        "#,
        );
        chunk.call::<()>(()).unwrap();

        // Create bookmark item
        let bookmark_item = lua.create_table().unwrap();
        bookmark_item.set("provider", "bookmarks").unwrap();
        bookmark_item.set("name", "Test Bookmark").unwrap();

        let actions = get_actions_for_item(&lua, &bookmark_item).unwrap();
        assert_eq!(actions.len(), 1);

        // Create non-bookmark item
        let app_item = lua.create_table().unwrap();
        app_item.set("provider", "applications").unwrap();
        app_item.set("name", "Test App").unwrap();

        let actions2 = get_actions_for_item(&lua, &app_item).unwrap();
        assert_eq!(actions2.len(), 0); // Filtered out
    }

    #[test]
    fn test_action_unregister() {
        let lua = setup_lua("test-plugin");

        let chunk = lua.load(
            r#"
            owlry.action.register({
                id = "temp-action",
                name = "Temporary",
                handler = function(item) end
            })
            return owlry.action.unregister("temp-action")
        "#,
        );
        let unregistered: bool = chunk.call(()).unwrap();
        assert!(unregistered);

        let actions = get_actions(&lua).unwrap();
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_execute_action() {
        let lua = setup_lua("test-plugin");

        // Register action that sets a global
        let chunk = lua.load(
            r#"
            result = nil
            owlry.action.register({
                id = "test-exec",
                name = "Test Execute",
                handler = function(item)
                    result = item.name
                end
            })
        "#,
        );
        chunk.call::<()>(()).unwrap();

        // Create test item
        let item = lua.create_table().unwrap();
        item.set("name", "TestItem").unwrap();

        // Execute action
        execute_action(&lua, "test-plugin:test-exec", &item).unwrap();

        // Verify handler was called
        let result: String = lua.globals().get("result").unwrap();
        assert_eq!(result, "TestItem");
    }
}
