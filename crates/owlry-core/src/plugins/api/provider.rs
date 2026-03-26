//! Provider registration API for Lua plugins
//!
//! Allows plugins to register providers via `owlry.provider.register()`

use mlua::{Function, Lua, Result as LuaResult, Table};

/// Provider registration data extracted from Lua
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some fields are for future use
pub struct ProviderRegistration {
    /// Provider name (used for filtering/identification)
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Provider type ID (for badge/filtering)
    pub type_id: String,
    /// Default icon name
    pub default_icon: String,
    /// Whether this is a static provider (refresh once) or dynamic (query-based)
    pub is_static: bool,
    /// Prefix to trigger this provider (e.g., ":" for commands)
    pub prefix: Option<String>,
}

/// Register owlry.provider.* API
pub fn register_provider_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let provider_table = lua.create_table()?;

    // Initialize registry for storing provider registrations
    let registrations: Table = lua.create_table()?;
    lua.set_named_registry_value("provider_registrations", registrations)?;

    // owlry.provider.register(config) - Register a new provider
    provider_table.set(
        "register",
        lua.create_function(|lua, config: Table| {
            // Extract required fields
            let name: String = config
                .get("name")
                .map_err(|_| mlua::Error::external("provider.register: 'name' is required"))?;

            let _display_name: String = config.get("display_name").unwrap_or_else(|_| name.clone());

            let type_id: String = config
                .get("type_id")
                .unwrap_or_else(|_| name.replace('-', "_"));

            let _default_icon: String = config
                .get("default_icon")
                .unwrap_or_else(|_| "application-x-executable".to_string());

            let _prefix: Option<String> = config.get("prefix").ok();

            // Check for refresh function (static provider) or query function (dynamic)
            let has_refresh = config.get::<Function>("refresh").is_ok();
            let has_query = config.get::<Function>("query").is_ok();

            if !has_refresh && !has_query {
                return Err(mlua::Error::external(
                    "provider.register: either 'refresh' or 'query' function is required",
                ));
            }

            let is_static = has_refresh;

            log::info!(
                "[plugin] Registered provider '{}' (type: {}, static: {})",
                name,
                type_id,
                is_static
            );

            // Store the config in registry for later retrieval
            let registrations: Table = lua.named_registry_value("provider_registrations")?;
            registrations.set(name.clone(), config)?;

            Ok(name)
        })?,
    )?;

    owlry.set("provider", provider_table)?;
    Ok(())
}

/// Get all provider registrations from the Lua runtime
pub fn get_registrations(lua: &Lua) -> LuaResult<Vec<ProviderRegistration>> {
    let registrations: Table = lua.named_registry_value("provider_registrations")?;
    let mut result = Vec::new();

    for pair in registrations.pairs::<String, Table>() {
        let (name, config) = pair?;

        let display_name: String = config.get("display_name").unwrap_or_else(|_| name.clone());
        let type_id: String = config
            .get("type_id")
            .unwrap_or_else(|_| name.replace('-', "_"));
        let default_icon: String = config
            .get("default_icon")
            .unwrap_or_else(|_| "application-x-executable".to_string());
        let prefix: Option<String> = config.get("prefix").ok();
        let is_static = config.get::<Function>("refresh").is_ok();

        result.push(ProviderRegistration {
            name,
            display_name,
            type_id,
            default_icon,
            is_static,
            prefix,
        });
    }

    Ok(result)
}

/// Call a provider's refresh function and extract items
pub fn call_refresh(lua: &Lua, provider_name: &str) -> LuaResult<Vec<PluginItem>> {
    let registrations: Table = lua.named_registry_value("provider_registrations")?;
    let config: Table = registrations.get(provider_name)?;
    let refresh: Function = config.get("refresh")?;

    let items: Table = refresh.call(())?;
    extract_items(&items)
}

/// Call a provider's query function with a query string
#[allow(dead_code)] // Will be used for dynamic query providers
pub fn call_query(lua: &Lua, provider_name: &str, query: &str) -> LuaResult<Vec<PluginItem>> {
    let registrations: Table = lua.named_registry_value("provider_registrations")?;
    let config: Table = registrations.get(provider_name)?;
    let query_fn: Function = config.get("query")?;

    let items: Table = query_fn.call(query.to_string())?;
    extract_items(&items)
}

/// Item data from a plugin provider
#[derive(Debug, Clone)]
#[allow(dead_code)] // data field is for future action handlers
pub struct PluginItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub command: Option<String>,
    pub terminal: bool,
    pub tags: Vec<String>,
    /// Custom data passed to action handlers
    pub data: Option<String>,
}

/// Extract items from a Lua table returned by refresh/query
fn extract_items(items: &Table) -> LuaResult<Vec<PluginItem>> {
    let mut result = Vec::new();

    for pair in items.clone().pairs::<i64, Table>() {
        let (_, item) = pair?;

        let id: String = item.get("id")?;
        let name: String = item.get("name")?;
        let description: Option<String> = item.get("description").ok();
        let icon: Option<String> = item.get("icon").ok();
        let command: Option<String> = item.get("command").ok();
        let terminal: bool = item.get("terminal").unwrap_or(false);
        let data: Option<String> = item.get("data").ok();

        // Extract tags array
        let tags: Vec<String> = if let Ok(tags_table) = item.get::<Table>("tags") {
            tags_table
                .pairs::<i64, String>()
                .filter_map(|r| r.ok())
                .map(|(_, v)| v)
                .collect()
        } else {
            Vec::new()
        };

        result.push(PluginItem {
            id,
            name,
            description,
            icon,
            command,
            terminal,
            tags,
            data,
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_lua() -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_provider_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_register_static_provider() {
        let lua = create_test_lua();

        let script = r#"
            owlry.provider.register({
                name = "test-provider",
                display_name = "Test Provider",
                type_id = "test",
                default_icon = "test-icon",
                refresh = function()
                    return {
                        { id = "1", name = "Item 1", description = "First item" },
                        { id = "2", name = "Item 2", command = "echo hello" },
                    }
                end
            })
        "#;
        lua.load(script).call::<()>(()).unwrap();

        let registrations = get_registrations(&lua).unwrap();
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].name, "test-provider");
        assert_eq!(registrations[0].display_name, "Test Provider");
        assert!(registrations[0].is_static);
    }

    #[test]
    fn test_register_dynamic_provider() {
        let lua = create_test_lua();

        let script = r#"
            owlry.provider.register({
                name = "search",
                prefix = "?",
                query = function(q)
                    return {
                        { id = "result", name = "Result for: " .. q }
                    }
                end
            })
        "#;
        lua.load(script).call::<()>(()).unwrap();

        let registrations = get_registrations(&lua).unwrap();
        assert_eq!(registrations.len(), 1);
        assert!(!registrations[0].is_static);
        assert_eq!(registrations[0].prefix, Some("?".to_string()));
    }

    #[test]
    fn test_call_refresh() {
        let lua = create_test_lua();

        let script = r#"
            owlry.provider.register({
                name = "items",
                refresh = function()
                    return {
                        { id = "a", name = "Alpha", tags = {"one", "two"} },
                        { id = "b", name = "Beta", terminal = true },
                    }
                end
            })
        "#;
        lua.load(script).call::<()>(()).unwrap();

        let items = call_refresh(&lua, "items").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "a");
        assert_eq!(items[0].name, "Alpha");
        assert_eq!(items[0].tags, vec!["one", "two"]);
        assert!(!items[0].terminal);
        assert_eq!(items[1].id, "b");
        assert!(items[1].terminal);
    }

    #[test]
    fn test_call_query() {
        let lua = create_test_lua();

        let script = r#"
            owlry.provider.register({
                name = "search",
                query = function(q)
                    return {
                        { id = "1", name = "Found: " .. q }
                    }
                end
            })
        "#;
        lua.load(script).call::<()>(()).unwrap();

        let items = call_query(&lua, "search", "hello").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Found: hello");
    }

    #[test]
    fn test_register_missing_function() {
        let lua = create_test_lua();

        let script = r#"
            owlry.provider.register({
                name = "broken",
            })
        "#;
        let result = lua.load(script).call::<()>(());
        assert!(result.is_err());
    }
}
