//! Theme API for Lua plugins
//!
//! Allows plugins to contribute CSS themes:
//! - `owlry.theme.register(config)` - Register a theme

use mlua::{Lua, Result as LuaResult, Table, Value};
use std::path::Path;

/// Theme registration data
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used by theme loading
pub struct ThemeRegistration {
    /// Theme name (used in config)
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// CSS content
    pub css: String,
    /// Plugin that registered this theme
    pub plugin_id: String,
}

/// Register theme APIs
pub fn register_theme_api(
    lua: &Lua,
    owlry: &Table,
    plugin_id: &str,
    plugin_dir: &Path,
) -> LuaResult<()> {
    let theme_table = lua.create_table()?;
    let plugin_id_owned = plugin_id.to_string();
    let plugin_dir_owned = plugin_dir.to_path_buf();

    // Initialize theme storage in Lua registry
    if lua.named_registry_value::<Value>("themes")?.is_nil() {
        let themes: Table = lua.create_table()?;
        lua.set_named_registry_value("themes", themes)?;
    }

    // owlry.theme.register(config) -> string (theme_name)
    // config = {
    //   name = "dark-owl",
    //   display_name = "Dark Owl",  -- optional, defaults to name
    //   css = "...",  -- CSS string
    //   -- OR
    //   css_file = "theme.css"  -- path relative to plugin dir
    // }
    let plugin_id_for_register = plugin_id_owned.clone();
    let plugin_dir_for_register = plugin_dir_owned.clone();
    theme_table.set(
        "register",
        lua.create_function(move |lua, config: Table| {
            // Extract required fields
            let name: String = config
                .get("name")
                .map_err(|_| mlua::Error::external("theme.register: 'name' is required"))?;

            let display_name: String = config.get("display_name").unwrap_or_else(|_| name.clone());

            // Get CSS either directly or from file
            let css: String = if let Ok(css_str) = config.get::<String>("css") {
                css_str
            } else if let Ok(css_file) = config.get::<String>("css_file") {
                let css_path = plugin_dir_for_register.join(&css_file);
                std::fs::read_to_string(&css_path).map_err(|e| {
                    mlua::Error::external(format!(
                        "Failed to read CSS file '{}': {}",
                        css_path.display(),
                        e
                    ))
                })?
            } else {
                return Err(mlua::Error::external(
                    "theme.register: either 'css' or 'css_file' is required",
                ));
            };

            // Store theme in registry
            let themes: Table = lua.named_registry_value("themes")?;

            let theme_entry = lua.create_table()?;
            theme_entry.set("name", name.clone())?;
            theme_entry.set("display_name", display_name.clone())?;
            theme_entry.set("css", css)?;
            theme_entry.set("plugin_id", plugin_id_for_register.clone())?;

            themes.set(name.clone(), theme_entry)?;

            log::info!(
                "[plugin:{}] Registered theme '{}'",
                plugin_id_for_register,
                name
            );

            Ok(name)
        })?,
    )?;

    // owlry.theme.unregister(name) -> boolean
    theme_table.set(
        "unregister",
        lua.create_function(|lua, name: String| {
            let themes: Table = lua.named_registry_value("themes")?;

            if themes.contains_key(name.clone())? {
                themes.set(name, Value::Nil)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?,
    )?;

    // owlry.theme.list() -> table of theme names
    theme_table.set(
        "list",
        lua.create_function(|lua, ()| {
            let themes: Table = match lua.named_registry_value("themes") {
                Ok(t) => t,
                Err(_) => return lua.create_table(),
            };

            let result = lua.create_table()?;
            let mut i = 1;

            for pair in themes.pairs::<String, Table>() {
                let (name, _) = pair?;
                result.set(i, name)?;
                i += 1;
            }

            Ok(result)
        })?,
    )?;

    owlry.set("theme", theme_table)?;
    Ok(())
}

/// Get all registered themes from a Lua runtime
#[allow(dead_code)] // Will be used by theme system
pub fn get_themes(lua: &Lua) -> LuaResult<Vec<ThemeRegistration>> {
    let themes: Table = match lua.named_registry_value("themes") {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let mut result = Vec::new();

    for pair in themes.pairs::<String, Table>() {
        let (_, entry) = pair?;

        let name: String = entry.get("name")?;
        let display_name: String = entry.get("display_name")?;
        let css: String = entry.get("css")?;
        let plugin_id: String = entry.get("plugin_id")?;

        result.push(ThemeRegistration {
            name,
            display_name,
            css,
            plugin_id,
        });
    }

    Ok(result)
}

/// Get a specific theme's CSS by name
#[allow(dead_code)] // Will be used by theme loading
pub fn get_theme_css(lua: &Lua, name: &str) -> LuaResult<Option<String>> {
    let themes: Table = match lua.named_registry_value("themes") {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    if let Ok(entry) = themes.get::<Table>(name) {
        let css: String = entry.get("css")?;
        Ok(Some(css))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_lua(plugin_id: &str, plugin_dir: &Path) -> Lua {
        let lua = Lua::new();
        let owlry = lua.create_table().unwrap();
        register_theme_api(&lua, &owlry, plugin_id, plugin_dir).unwrap();
        lua.globals().set("owlry", owlry).unwrap();
        lua
    }

    #[test]
    fn test_theme_registration_inline() {
        let temp = TempDir::new().unwrap();
        let lua = setup_lua("test-plugin", temp.path());

        let chunk = lua.load(
            r#"
            return owlry.theme.register({
                name = "my-theme",
                display_name = "My Theme",
                css = ".owlry-window { background: #333; }"
            })
        "#,
        );
        let name: String = chunk.call(()).unwrap();
        assert_eq!(name, "my-theme");

        let themes = get_themes(&lua).unwrap();
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].display_name, "My Theme");
        assert!(themes[0].css.contains("background: #333"));
    }

    #[test]
    fn test_theme_registration_file() {
        let temp = TempDir::new().unwrap();
        let css_content = ".owlry-window { background: #444; }";
        std::fs::write(temp.path().join("theme.css"), css_content).unwrap();

        let lua = setup_lua("test-plugin", temp.path());

        let chunk = lua.load(
            r#"
            return owlry.theme.register({
                name = "file-theme",
                css_file = "theme.css"
            })
        "#,
        );
        let name: String = chunk.call(()).unwrap();
        assert_eq!(name, "file-theme");

        let css = get_theme_css(&lua, "file-theme").unwrap();
        assert!(css.is_some());
        assert!(css.unwrap().contains("background: #444"));
    }

    #[test]
    fn test_theme_list() {
        let temp = TempDir::new().unwrap();
        let lua = setup_lua("test-plugin", temp.path());

        let chunk = lua.load(
            r#"
            owlry.theme.register({ name = "theme1", css = "a{}" })
            owlry.theme.register({ name = "theme2", css = "b{}" })
            return owlry.theme.list()
        "#,
        );
        let list: Table = chunk.call(()).unwrap();

        let mut names: Vec<String> = Vec::new();
        for pair in list.pairs::<i64, String>() {
            let (_, name) = pair.unwrap();
            names.push(name);
        }
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"theme1".to_string()));
        assert!(names.contains(&"theme2".to_string()));
    }

    #[test]
    fn test_theme_unregister() {
        let temp = TempDir::new().unwrap();
        let lua = setup_lua("test-plugin", temp.path());

        let chunk = lua.load(
            r#"
            owlry.theme.register({ name = "temp-theme", css = "c{}" })
            return owlry.theme.unregister("temp-theme")
        "#,
        );
        let unregistered: bool = chunk.call(()).unwrap();
        assert!(unregistered);

        let themes = get_themes(&lua).unwrap();
        assert_eq!(themes.len(), 0);
    }
}
