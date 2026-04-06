//! Provider registration API for Lua plugins

use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use owlry_plugin_api::PluginItem;
use std::cell::RefCell;

use crate::loader::ProviderRegistration;

thread_local! {
    static REGISTRATIONS: RefCell<Vec<ProviderRegistration>> = const { RefCell::new(Vec::new()) };
}

/// Register the provider API in the owlry table
pub fn register_provider_api(lua: &Lua, owlry: &Table) -> LuaResult<()> {
    let provider = lua.create_table()?;

    // owlry.provider.register(config)
    provider.set("register", lua.create_function(register_provider)?)?;

    owlry.set("provider", provider)?;
    Ok(())
}

/// Implementation of owlry.provider.register()
fn register_provider(lua: &Lua, config: Table) -> LuaResult<()> {
    let name: String = config.get("name")?;
    let display_name: String = config
        .get::<Option<String>>("display_name")?
        .unwrap_or_else(|| name.clone());
    let type_id: String = config
        .get::<Option<String>>("type_id")?
        .unwrap_or_else(|| name.replace('-', "_"));
    let default_icon: String = config
        .get::<Option<String>>("default_icon")?
        .unwrap_or_else(|| "application-x-addon".to_string());
    let prefix: Option<String> = config.get("prefix")?;

    // Check if it's a dynamic provider (has query function) or static (has refresh)
    let has_query: bool = config.contains_key("query")?;
    let has_refresh: bool = config.contains_key("refresh")?;

    if !has_query && !has_refresh {
        return Err(mlua::Error::external(
            "Provider must have either 'refresh' or 'query' function",
        ));
    }

    let is_dynamic = has_query;

    // Store the config table in owlry.provider._registrations[name]
    // so call_refresh/call_query can find the callback functions later
    let globals = lua.globals();
    let owlry: Table = globals.get("owlry")?;
    let provider: Table = owlry.get("provider")?;
    let registrations: Table = match provider.get::<Value>("_registrations")? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            provider.set("_registrations", t.clone())?;
            t
        }
    };
    registrations.set(name.as_str(), config)?;

    REGISTRATIONS.with(|regs| {
        regs.borrow_mut().push(ProviderRegistration {
            name,
            display_name,
            type_id,
            default_icon,
            prefix,
            is_dynamic,
        });
    });

    Ok(())
}

/// Call the top-level `refresh()` global function (for manifest-declared providers)
pub fn call_global_refresh(lua: &Lua) -> LuaResult<Vec<PluginItem>> {
    let globals = lua.globals();
    match globals.get::<Function>("refresh") {
        Ok(refresh_fn) => parse_items_result(refresh_fn.call(())?),
        Err(_) => Ok(Vec::new()),
    }
}

/// Get all registered providers
pub fn get_registrations(lua: &Lua) -> LuaResult<Vec<ProviderRegistration>> {
    // Suppress unused warning
    let _ = lua;

    REGISTRATIONS.with(|regs| Ok(regs.borrow().clone()))
}

/// Call a provider's refresh function
pub fn call_refresh(lua: &Lua, provider_name: &str) -> LuaResult<Vec<PluginItem>> {
    let globals = lua.globals();
    let owlry: Table = globals.get("owlry")?;
    let provider: Table = owlry.get("provider")?;

    // Get the registered providers table (internal)
    let registrations: Table = match provider.get::<Value>("_registrations")? {
        Value::Table(t) => t,
        _ => {
            // Try to find the config directly from the global scope
            // This happens when register was called with the config table
            return call_provider_function(lua, provider_name, "refresh", None);
        }
    };

    let config: Table = match registrations.get(provider_name)? {
        Value::Table(t) => t,
        _ => return Ok(Vec::new()),
    };

    let refresh_fn: Function = match config.get("refresh")? {
        Value::Function(f) => f,
        _ => return Ok(Vec::new()),
    };

    let result: Value = refresh_fn.call(())?;
    parse_items_result(result)
}

/// Call a provider's query function
pub fn call_query(lua: &Lua, provider_name: &str, query: &str) -> LuaResult<Vec<PluginItem>> {
    call_provider_function(lua, provider_name, "query", Some(query))
}

/// Call a provider function by name
fn call_provider_function(
    lua: &Lua,
    provider_name: &str,
    function_name: &str,
    query: Option<&str>,
) -> LuaResult<Vec<PluginItem>> {
    // Search through all registered providers in the Lua globals
    // This is a workaround since we store registrations thread-locally
    let globals = lua.globals();

    // Try to find a registered provider with matching name
    // First check if there's a _providers table
    if let Ok(Value::Table(providers)) = globals.get::<Value>("_owlry_providers")
        && let Ok(Value::Table(config)) = providers.get::<Value>(provider_name)
        && let Ok(Value::Function(func)) = config.get::<Value>(function_name)
    {
        let result: Value = match query {
            Some(q) => func.call(q)?,
            None => func.call(())?,
        };
        return parse_items_result(result);
    }

    // Fall back: search through globals for functions
    // This is less reliable but handles simple cases
    Ok(Vec::new())
}

/// Parse items from Lua return value
fn parse_items_result(result: Value) -> LuaResult<Vec<PluginItem>> {
    let mut items = Vec::new();

    if let Value::Table(table) = result {
        for pair in table.pairs::<i32, Table>() {
            let (_, item_table) = pair?;
            if let Ok(item) = parse_item(&item_table) {
                items.push(item);
            }
        }
    }

    Ok(items)
}

/// Parse a single item from a Lua table
fn parse_item(table: &Table) -> LuaResult<PluginItem> {
    let id: String = table.get("id")?;
    let name: String = table.get("name")?;
    let command: String = table.get::<Option<String>>("command")?.unwrap_or_default();
    let description: Option<String> = table.get("description")?;
    let icon: Option<String> = table.get("icon")?;
    let terminal: bool = table.get::<Option<bool>>("terminal")?.unwrap_or(false);
    let tags: Vec<String> = table
        .get::<Option<Vec<String>>>("tags")?
        .unwrap_or_default();

    let mut item = PluginItem::new(id, name, command);

    if let Some(desc) = description {
        item = item.with_description(desc);
    }
    if let Some(ic) = icon {
        item = item.with_icon(&ic);
    }
    if terminal {
        item = item.with_terminal(true);
    }
    if !tags.is_empty() {
        item = item.with_keywords(tags);
    }

    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{SandboxConfig, create_lua_runtime};

    #[test]
    fn test_register_static_provider() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();

        let owlry = lua.create_table().unwrap();
        register_provider_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        let code = r#"
            owlry.provider.register({
                name = "test-provider",
                display_name = "Test Provider",
                refresh = function()
                    return {
                        { id = "1", name = "Item 1" }
                    }
                end
            })
        "#;
        lua.load(code).set_name("test").call::<()>(()).unwrap();

        let regs = get_registrations(&lua).unwrap();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].name, "test-provider");
        assert!(!regs[0].is_dynamic);
    }

    #[test]
    fn test_register_dynamic_provider() {
        let config = SandboxConfig::default();
        let lua = create_lua_runtime(&config).unwrap();

        let owlry = lua.create_table().unwrap();
        register_provider_api(&lua, &owlry).unwrap();
        lua.globals().set("owlry", owlry).unwrap();

        let code = r#"
            owlry.provider.register({
                name = "query-provider",
                prefix = "?",
                query = function(q)
                    return {
                        { id = "search", name = "Search: " .. q }
                    }
                end
            })
        "#;
        lua.load(code).set_name("test").call::<()>(()).unwrap();

        let regs = get_registrations(&lua).unwrap();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].name, "query-provider");
        assert!(regs[0].is_dynamic);
        assert_eq!(regs[0].prefix, Some("?".to_string()));
    }
}
