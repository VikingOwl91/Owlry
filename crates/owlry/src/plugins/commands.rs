//! Plugin CLI command implementations
//!
//! This module provides handlers for the `owlry plugin` subcommands.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::{PluginCommand as CliPluginCommand, PluginRuntime};
use crate::config::Config;
use crate::paths;
use crate::plugins::manifest::{discover_plugins, PluginManifest};
use crate::plugins::registry::{self, RegistryClient};
use crate::plugins::runtime_loader::{lua_runtime_available, rune_runtime_available};

/// Result type for plugin commands
pub type CommandResult = Result<(), String>;

/// Get registry client with configured URL
fn get_registry_client() -> RegistryClient {
    let config = Config::load().unwrap_or_default();
    match &config.plugins.registry_url {
        Some(url) => RegistryClient::new(url),
        None => RegistryClient::default_registry(),
    }
}

/// Check if a runtime is available
fn check_runtime_available(runtime: PluginRuntime) -> CommandResult {
    match runtime {
        PluginRuntime::Lua if !lua_runtime_available() => {
            Err("Lua runtime not installed. Install the owlry-lua package.".to_string())
        }
        PluginRuntime::Rune if !rune_runtime_available() => {
            Err("Rune runtime not installed. Install the owlry-rune package.".to_string())
        }
        _ => Ok(()),
    }
}

/// Check if any script runtime is available
fn any_runtime_available() -> bool {
    lua_runtime_available() || rune_runtime_available()
}

/// Execute a plugin command
pub fn execute(cmd: CliPluginCommand) -> CommandResult {
    match cmd {
        CliPluginCommand::List { enabled, disabled, runtime, available, refresh, json } => {
            if available {
                cmd_list_available(refresh, json)
            } else {
                cmd_list_installed(enabled, disabled, runtime, json)
            }
        }
        CliPluginCommand::Search { query, refresh, json } => cmd_search(&query, refresh, json),
        CliPluginCommand::Info { name, registry, json } => {
            if registry {
                cmd_info_registry(&name, json)
            } else {
                cmd_info_installed(&name, json)
            }
        }
        CliPluginCommand::Install { source, force } => {
            if !any_runtime_available() {
                return Err(
                    "No script runtime installed. Install owlry-lua or owlry-rune to use plugins."
                        .to_string(),
                );
            }
            cmd_install(&source, force)
        }
        CliPluginCommand::Remove { name, yes } => cmd_remove(&name, yes),
        CliPluginCommand::Update { name } => cmd_update(name.as_deref()),
        CliPluginCommand::Enable { name } => cmd_enable(&name),
        CliPluginCommand::Disable { name } => cmd_disable(&name),
        CliPluginCommand::Create { name, runtime, dir, display_name, description } => {
            check_runtime_available(runtime)?;
            cmd_create(&name, runtime, dir.as_deref(), display_name.as_deref(), description.as_deref())
        }
        CliPluginCommand::Validate { path } => cmd_validate(path.as_deref()),
        CliPluginCommand::Runtimes => cmd_runtimes(),
        CliPluginCommand::Run { plugin_id, command, args } => {
            cmd_run_plugin_command(&plugin_id, &command, &args)
        }
        CliPluginCommand::Commands { plugin_id } => cmd_list_commands(plugin_id.as_deref()),
    }
}

/// Detect the runtime type for a plugin based on entry point extension
fn detect_runtime(manifest: &PluginManifest) -> PluginRuntime {
    let entry = &manifest.plugin.entry;
    if entry.ends_with(".rn") {
        PluginRuntime::Rune
    } else {
        // Default to Lua for .lua or unrecognized extensions
        PluginRuntime::Lua
    }
}

/// List installed plugins
fn cmd_list_installed(
    only_enabled: bool,
    only_disabled: bool,
    runtime_filter: Option<PluginRuntime>,
    json_output: bool,
) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;

    if !plugins_dir.exists() {
        if json_output {
            println!("[]");
        } else {
            println!("No plugins installed.");
        }
        return Ok(());
    }

    let discovered = discover_plugins(&plugins_dir).map_err(|e| e.to_string())?;
    let config = Config::load().unwrap_or_default();
    let disabled_list = &config.plugins.disabled_plugins;

    let lua_available = lua_runtime_available();
    let rune_available = rune_runtime_available();

    let mut plugins: Vec<_> = discovered
        .iter()
        .map(|(id, (manifest, _path))| {
            let is_disabled = disabled_list.contains(id);
            let runtime = detect_runtime(manifest);
            (id.clone(), manifest.clone(), is_disabled, runtime)
        })
        .collect();

    // Apply filters
    if only_enabled {
        plugins.retain(|(_, _, is_disabled, _)| !*is_disabled);
    }
    if only_disabled {
        plugins.retain(|(_, _, is_disabled, _)| *is_disabled);
    }
    if let Some(rt) = runtime_filter {
        plugins.retain(|(_, _, _, runtime)| *runtime == rt);
    }

    // Sort by ID
    plugins.sort_by(|a, b| a.0.cmp(&b.0));

    if json_output {
        let json_list: Vec<_> = plugins
            .iter()
            .map(|(id, manifest, is_disabled, runtime)| {
                let runtime_available = match runtime {
                    PluginRuntime::Lua => lua_available,
                    PluginRuntime::Rune => rune_available,
                };
                serde_json::json!({
                    "id": id,
                    "name": manifest.plugin.name,
                    "version": manifest.plugin.version,
                    "description": manifest.plugin.description,
                    "enabled": !is_disabled,
                    "runtime": runtime.to_string(),
                    "runtime_available": runtime_available,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_list).unwrap());
    } else if plugins.is_empty() {
        println!("No plugins found.");
    } else {
        println!("Installed plugins:\n");
        for (id, manifest, is_disabled, runtime) in &plugins {
            let status = if *is_disabled { " (disabled)" } else { "" };
            let runtime_available = match runtime {
                PluginRuntime::Lua => lua_available,
                PluginRuntime::Rune => rune_available,
            };
            let runtime_status = if !runtime_available {
                format!(" [{} - NOT INSTALLED]", runtime)
            } else {
                format!(" [{}]", runtime)
            };
            println!(
                "  {} v{}{}{}\n    {}",
                id,
                manifest.plugin.version,
                status,
                runtime_status,
                if manifest.plugin.description.is_empty() {
                    "No description"
                } else {
                    &manifest.plugin.description
                }
            );
        }
        println!("\n{} plugin(s) installed.", plugins.len());
    }

    Ok(())
}

/// List available plugins from registry
fn cmd_list_available(refresh: bool, json_output: bool) -> CommandResult {
    let client = get_registry_client();

    println!("Fetching plugin list from registry...");
    let plugins = client.list_all(refresh)?;

    if json_output {
        let json_list: Vec<_> = plugins
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "version": p.version,
                    "description": p.description,
                    "author": p.author,
                    "repository": p.repository,
                    "tags": p.tags,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_list).unwrap());
    } else if plugins.is_empty() {
        println!("No plugins available in registry.");
    } else {
        println!("Available plugins:\n");
        for p in &plugins {
            println!(
                "  {} v{}\n    {}",
                p.id,
                p.version,
                if p.description.is_empty() {
                    "No description"
                } else {
                    &p.description
                }
            );
            if !p.tags.is_empty() {
                println!("    Tags: {}", p.tags.join(", "));
            }
        }
        println!("\n{} plugin(s) available.", plugins.len());
        println!("\nInstall with: owlry plugin install <name>");
    }

    Ok(())
}

/// Search for plugins in registry
fn cmd_search(query: &str, refresh: bool, json_output: bool) -> CommandResult {
    let client = get_registry_client();

    let plugins = client.search(query, refresh)?;

    if json_output {
        let json_list: Vec<_> = plugins
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "version": p.version,
                    "description": p.description,
                    "author": p.author,
                    "repository": p.repository,
                    "tags": p.tags,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_list).unwrap());
    } else if plugins.is_empty() {
        println!("No plugins found matching '{}'.", query);
    } else {
        println!("Search results for '{}':\n", query);
        for p in &plugins {
            println!(
                "  {} v{}\n    {}",
                p.id,
                p.version,
                if p.description.is_empty() {
                    "No description"
                } else {
                    &p.description
                }
            );
            if !p.tags.is_empty() {
                println!("    Tags: {}", p.tags.join(", "));
            }
        }
        println!("\n{} result(s) found.", plugins.len());
        println!("\nInstall with: owlry plugin install <name>");
    }

    Ok(())
}

/// Show information about an installed plugin
fn cmd_info_installed(name: &str, json_output: bool) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;
    let plugin_path = plugins_dir.join(name);

    if !plugin_path.exists() {
        return Err(format!("Plugin '{}' not found", name));
    }

    let manifest_path = plugin_path.join("plugin.toml");
    if !manifest_path.exists() {
        return Err(format!("Plugin '{}' has no manifest", name));
    }

    let manifest_content = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: PluginManifest = toml::from_str(&manifest_content)
        .map_err(|e| format!("Failed to parse manifest: {}", e))?;

    let config = Config::load().unwrap_or_default();
    let is_enabled = !config.plugins.disabled_plugins.contains(&name.to_string());

    let runtime = detect_runtime(&manifest);
    let runtime_available = match runtime {
        PluginRuntime::Lua => lua_runtime_available(),
        PluginRuntime::Rune => rune_runtime_available(),
    };

    if json_output {
        let info = serde_json::json!({
            "id": manifest.plugin.id,
            "name": manifest.plugin.name,
            "version": manifest.plugin.version,
            "description": manifest.plugin.description,
            "author": manifest.plugin.author,
            "owlry_version": manifest.plugin.owlry_version,
            "enabled": is_enabled,
            "runtime": runtime.to_string(),
            "runtime_available": runtime_available,
            "path": plugin_path.display().to_string(),
            "provides": {
                "providers": manifest.provides.providers,
                "actions": manifest.provides.actions,
                "themes": manifest.provides.themes,
                "hooks": manifest.provides.hooks,
            },
            "permissions": {
                "network": manifest.permissions.network,
                "filesystem": manifest.permissions.filesystem,
                "run_commands": manifest.permissions.run_commands,
            }
        });
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!("Plugin: {} v{}", manifest.plugin.name, manifest.plugin.version);
        println!("ID: {}", manifest.plugin.id);
        if !manifest.plugin.description.is_empty() {
            println!("Description: {}", manifest.plugin.description);
        }
        if !manifest.plugin.author.is_empty() {
            println!("Author: {}", manifest.plugin.author);
        }
        println!("Status: {}", if is_enabled { "enabled" } else { "disabled" });
        println!(
            "Runtime: {}{}",
            runtime,
            if runtime_available { "" } else { " (NOT INSTALLED)" }
        );
        println!("Path: {}", plugin_path.display());
        println!();
        println!("Provides:");
        if !manifest.provides.providers.is_empty() {
            println!("  Providers: {}", manifest.provides.providers.join(", "));
        }
        if manifest.provides.actions {
            println!("  Actions: yes");
        }
        if !manifest.provides.themes.is_empty() {
            println!("  Themes: {}", manifest.provides.themes.join(", "));
        }
        if manifest.provides.hooks {
            println!("  Hooks: yes");
        }
        println!();
        println!("Permissions:");
        println!("  Network: {}", if manifest.permissions.network { "yes" } else { "no" });
        if !manifest.permissions.filesystem.is_empty() {
            println!("  Filesystem: {}", manifest.permissions.filesystem.join(", "));
        }
        if !manifest.permissions.run_commands.is_empty() {
            println!("  Commands: {}", manifest.permissions.run_commands.join(", "));
        }
    }

    Ok(())
}

/// Show information about a plugin from the registry
fn cmd_info_registry(name: &str, json_output: bool) -> CommandResult {
    let client = get_registry_client();

    let plugin = client.find(name, false)?
        .ok_or_else(|| format!("Plugin '{}' not found in registry", name))?;

    if json_output {
        let info = serde_json::json!({
            "id": plugin.id,
            "name": plugin.name,
            "version": plugin.version,
            "description": plugin.description,
            "author": plugin.author,
            "repository": plugin.repository,
            "tags": plugin.tags,
            "owlry_version": plugin.owlry_version,
            "license": plugin.license,
        });
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!("Plugin: {} v{}", plugin.name, plugin.version);
        println!("ID: {}", plugin.id);
        if !plugin.description.is_empty() {
            println!("Description: {}", plugin.description);
        }
        if !plugin.author.is_empty() {
            println!("Author: {}", plugin.author);
        }
        println!("Repository: {}", plugin.repository);
        if !plugin.owlry_version.is_empty() {
            println!("Requires: owlry {}", plugin.owlry_version);
        }
        if !plugin.license.is_empty() {
            println!("License: {}", plugin.license);
        }
        if !plugin.tags.is_empty() {
            println!("Tags: {}", plugin.tags.join(", "));
        }
        println!();
        println!("Install with: owlry plugin install {}", plugin.id);
    }

    Ok(())
}

/// Install a plugin from registry name, path, or URL
fn cmd_install(source: &str, force: bool) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;

    // Ensure plugins directory exists
    fs::create_dir_all(&plugins_dir)
        .map_err(|e| format!("Failed to create plugins directory: {}", e))?;

    // Determine source type: URL, path, or registry name
    if registry::is_url(source) {
        // Git repository URL
        install_from_git(source, &plugins_dir, force)
    } else if registry::is_path(source) {
        // Local path
        let source_path = PathBuf::from(source);
        install_from_path(&source_path, &plugins_dir, force)
    } else {
        // Try registry lookup
        println!("Looking up '{}' in registry...", source);
        let client = get_registry_client();

        match client.find(source, false)? {
            Some(plugin) => {
                println!("Found: {} v{}", plugin.name, plugin.version);
                install_from_git(&plugin.repository, &plugins_dir, force)
            }
            None => {
                Err(format!(
                    "Plugin '{}' not found in registry. Use a local path or git URL.",
                    source
                ))
            }
        }
    }
}

/// Install plugin from a local directory
fn install_from_path(source: &Path, plugins_dir: &Path, force: bool) -> CommandResult {
    // Validate source has manifest
    let manifest_path = source.join("plugin.toml");
    if !manifest_path.exists() {
        return Err("Source directory does not contain plugin.toml".to_string());
    }

    let manifest_content = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: PluginManifest = toml::from_str(&manifest_content)
        .map_err(|e| format!("Failed to parse manifest: {}", e))?;

    let target_dir = plugins_dir.join(&manifest.plugin.id);

    if target_dir.exists() {
        if force {
            println!("Removing existing plugin...");
            fs::remove_dir_all(&target_dir)
                .map_err(|e| format!("Failed to remove existing plugin: {}", e))?;
        } else {
            return Err(format!(
                "Plugin '{}' is already installed. Use --force to reinstall.",
                manifest.plugin.id
            ));
        }
    }

    // Copy plugin files
    copy_dir_recursive(source, &target_dir)?;

    println!(
        "Installed plugin '{}' v{} to {}",
        manifest.plugin.name,
        manifest.plugin.version,
        target_dir.display()
    );

    Ok(())
}

/// Install plugin from a git repository
fn install_from_git(url: &str, plugins_dir: &Path, force: bool) -> CommandResult {
    // Create temp directory for clone
    let temp_dir = std::env::temp_dir().join(format!("owlry-plugin-{}", std::process::id()));

    // Clean up temp dir if it exists
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to clean temp directory: {}", e))?;
    }

    println!("Cloning {}...", url);

    // Clone repository
    let status = std::process::Command::new("git")
        .args(["clone", "--depth=1", url, temp_dir.to_str().unwrap()])
        .status()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !status.success() {
        return Err("Git clone failed".to_string());
    }

    // Install from cloned directory
    let result = install_from_path(&temp_dir, plugins_dir, force);

    // Clean up temp directory
    let _ = fs::remove_dir_all(&temp_dir);

    result
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> CommandResult {
    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {}: {}", dst.display(), e))?;

    for entry in fs::read_dir(src).map_err(|e| format!("Failed to read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            // Skip .git directory
            if src_path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {}: {}", src_path.display(), e))?;
        }
    }

    Ok(())
}

/// Remove an installed plugin
fn cmd_remove(name: &str, yes: bool) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;
    let plugin_path = plugins_dir.join(name);

    if !plugin_path.exists() {
        return Err(format!("Plugin '{}' not found", name));
    }

    // Confirm unless --yes flag
    if !yes {
        print!("Remove plugin '{}'? [y/N] ", name);
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    fs::remove_dir_all(&plugin_path)
        .map_err(|e| format!("Failed to remove plugin: {}", e))?;

    // Also remove from disabled list if present
    if let Ok(mut config) = Config::load() {
        config.plugins.disabled_plugins.retain(|id| id != name);
        if let Err(e) = config.save() {
            eprintln!("Warning: Failed to update config: {}", e);
        }
    }

    println!("Removed plugin '{}'", name);
    Ok(())
}

/// Update plugins
fn cmd_update(name: Option<&str>) -> CommandResult {
    // For now, update is not implemented as we don't have source tracking
    // A full implementation would:
    // 1. Track where each plugin was installed from (git URL)
    // 2. Pull updates and reinstall
    if let Some(plugin) = name {
        println!("Update for plugin '{}' not yet implemented.", plugin);
        println!("To update manually, remove and reinstall the plugin.");
    } else {
        println!("Plugin updates not yet implemented.");
        println!("To update a plugin, remove and reinstall it.");
    }
    Ok(())
}

/// Enable a plugin
fn cmd_enable(name: &str) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;
    let plugin_path = plugins_dir.join(name);

    if !plugin_path.exists() {
        return Err(format!("Plugin '{}' not found", name));
    }

    let mut config = Config::load().unwrap_or_default();

    if !config.plugins.disabled_plugins.contains(&name.to_string()) {
        println!("Plugin '{}' is already enabled.", name);
        return Ok(());
    }

    config.plugins.disabled_plugins.retain(|id| id != name);
    config.save().map_err(|e| format!("Failed to save config: {}", e))?;

    println!("Enabled plugin '{}'", name);
    Ok(())
}

/// Disable a plugin
fn cmd_disable(name: &str) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;
    let plugin_path = plugins_dir.join(name);

    if !plugin_path.exists() {
        return Err(format!("Plugin '{}' not found", name));
    }

    let mut config = Config::load().unwrap_or_default();

    if config.plugins.disabled_plugins.contains(&name.to_string()) {
        println!("Plugin '{}' is already disabled.", name);
        return Ok(());
    }

    config.plugins.disabled_plugins.push(name.to_string());
    config.save().map_err(|e| format!("Failed to save config: {}", e))?;

    println!("Disabled plugin '{}'", name);
    Ok(())
}

/// Create a new plugin from template
fn cmd_create(
    name: &str,
    runtime: PluginRuntime,
    dir: Option<&str>,
    display_name: Option<&str>,
    description: Option<&str>,
) -> CommandResult {
    let base_dir = dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let plugin_dir = base_dir.join(name);

    if plugin_dir.exists() {
        return Err(format!("Directory '{}' already exists", plugin_dir.display()));
    }

    fs::create_dir_all(&plugin_dir)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    let display = display_name.unwrap_or(name);
    let desc = description.unwrap_or("A custom owlry plugin");

    let (entry_file, entry_ext) = match runtime {
        PluginRuntime::Lua => ("init.lua", "lua"),
        PluginRuntime::Rune => ("init.rn", "rn"),
    };

    // Create plugin.toml
    let manifest = format!(
        r#"[plugin]
id = "{name}"
name = "{display}"
version = "0.1.0"
description = "{desc}"
author = ""
owlry_version = ">=0.3.0"
entry = "{entry_file}"

[provides]
providers = ["{name}"]
actions = false
themes = []
hooks = false

[permissions]
network = false
filesystem = []
run_commands = []
"#,
        name = name,
        display = display,
        desc = desc,
        entry_file = entry_file,
    );

    fs::write(plugin_dir.join("plugin.toml"), manifest)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    // Create entry point template based on runtime
    match runtime {
        PluginRuntime::Lua => {
            let init_lua = format!(
                r#"-- {display} Plugin for Owlry
-- {desc}

-- Register the provider
owlry.provider.register({{
    name = "{name}",
    display_name = "{display}",
    type_id = "{name}",
    default_icon = "application-x-executable",

    refresh = function()
        -- Return a list of items
        return {{
            {{
                id = "{name}:example",
                name = "Example Item",
                description = "This is an example item from {display}",
                icon = "dialog-information",
                command = "echo 'Hello from {name}!'",
                terminal = false,
                tags = {{}}
            }}
        }}
    end
}})

owlry.log.info("{display} plugin loaded")
"#,
                name = name,
                display = display,
                desc = desc,
            );
            fs::write(plugin_dir.join(entry_file), init_lua)
                .map_err(|e| format!("Failed to write {}: {}", entry_file, e))?;
        }
        PluginRuntime::Rune => {
            // Note: Rune uses #{{ for object literals, so we build manually
            let init_rn = format!(
                r#"//! {display} Plugin for Owlry
//! {desc}

/// Plugin item structure
struct Item {{{{
    id: String,
    name: String,
    description: String,
    icon: String,
    command: String,
    terminal: bool,
    tags: Vec<String>,
}}}}

/// Provider registration
pub fn register(owlry) {{{{
    owlry.provider.register(#{{{{
        name: "{name}",
        display_name: "{display}",
        type_id: "{name}",
        default_icon: "application-x-executable",

        refresh: || {{{{
            // Return a list of items
            [
                Item {{{{
                    id: "{name}:example",
                    name: "Example Item",
                    description: "This is an example item from {display}",
                    icon: "dialog-information",
                    command: "echo 'Hello from {name}!'",
                    terminal: false,
                    tags: [],
                }}}},
            ]
        }}}},
    }}}});

    owlry.log.info("{display} plugin loaded");
}}}}
"#,
                name = name,
                display = display,
                desc = desc,
            );
            fs::write(plugin_dir.join(entry_file), init_rn)
                .map_err(|e| format!("Failed to write {}: {}", entry_file, e))?;
        }
    }

    println!("Created {} plugin '{}' at {}", runtime, name, plugin_dir.display());
    println!();
    println!("Next steps:");
    println!("  1. Edit {}/{} to implement your provider", name, entry_file);
    println!("  2. Install: owlry plugin install {}", plugin_dir.display());
    println!("  3. Test: owlry  (your plugin items should appear)");
    println!();
    println!("Runtime: {} (requires owlry-{} package)", runtime, entry_ext);

    Ok(())
}

/// Validate a plugin's structure
fn cmd_validate(path: Option<&str>) -> CommandResult {
    let plugin_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    println!("Validating plugin at {}...", plugin_path.display());

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Check manifest exists
    let manifest_path = plugin_path.join("plugin.toml");
    if !manifest_path.exists() {
        errors.push("Missing plugin.toml manifest".to_string());
    } else {
        // Parse and validate manifest
        match fs::read_to_string(&manifest_path) {
            Ok(content) => match toml::from_str::<PluginManifest>(&content) {
                Ok(manifest) => {
                    // Check required fields
                    if manifest.plugin.id.is_empty() {
                        errors.push("plugin.id is empty".to_string());
                    }
                    if manifest.plugin.name.is_empty() {
                        errors.push("plugin.name is empty".to_string());
                    }
                    if manifest.plugin.version.is_empty() {
                        errors.push("plugin.version is empty".to_string());
                    }

                    // Check entry point
                    let entry = &manifest.plugin.entry;
                    let entry_path = plugin_path.join(entry);
                    if !entry_path.exists() {
                        errors.push(format!("Entry point '{}' not found", entry));
                    }

                    // Validate owlry_version semver
                    if manifest.plugin.owlry_version.is_empty() {
                        warnings.push("No owlry_version specified".to_string());
                    } else if semver::VersionReq::parse(&manifest.plugin.owlry_version).is_err() {
                        errors.push(format!(
                            "Invalid owlry_version requirement: {}",
                            manifest.plugin.owlry_version
                        ));
                    }

                    // Check for empty provides
                    if manifest.provides.providers.is_empty()
                        && !manifest.provides.actions
                        && manifest.provides.themes.is_empty()
                        && !manifest.provides.hooks
                    {
                        warnings.push("Plugin does not provide any features".to_string());
                    }

                    println!("  Plugin ID: {}", manifest.plugin.id);
                    println!("  Version: {}", manifest.plugin.version);
                }
                Err(e) => {
                    errors.push(format!("Failed to parse manifest: {}", e));
                }
            },
            Err(e) => {
                errors.push(format!("Failed to read manifest: {}", e));
            }
        }
    }

    // Report results
    println!();

    if !warnings.is_empty() {
        println!("Warnings:");
        for w in &warnings {
            println!("  ⚠ {}", w);
        }
        println!();
    }

    if errors.is_empty() {
        println!("✓ Plugin is valid");
        Ok(())
    } else {
        println!("Errors:");
        for e in &errors {
            println!("  ✗ {}", e);
        }
        Err(format!("{} error(s) found", errors.len()))
    }
}

/// Show available script runtimes
fn cmd_runtimes() -> CommandResult {
    use crate::plugins::runtime_loader::SYSTEM_RUNTIMES_DIR;

    println!("Script Runtimes:\n");

    let lua_available = lua_runtime_available();
    let rune_available = rune_runtime_available();

    // Lua runtime
    if lua_available {
        println!("  ✓ Lua    - Installed");
        println!("              Package: owlry-lua");
        println!("              Entry point: init.lua");
    } else {
        println!("  ✗ Lua    - Not installed");
        println!("              Install: yay -S owlry-lua");
        println!("              Entry point: init.lua");
    }

    println!();

    // Rune runtime
    if rune_available {
        println!("  ✓ Rune   - Installed");
        println!("              Package: owlry-rune");
        println!("              Entry point: init.rn");
    } else {
        println!("  ✗ Rune   - Not installed");
        println!("              Install: yay -S owlry-rune");
        println!("              Entry point: init.rn");
    }

    println!();
    println!("Runtime directory: {}", SYSTEM_RUNTIMES_DIR);

    if !lua_available && !rune_available {
        println!();
        println!("No runtimes installed. Install at least one to use plugins:");
        println!("  yay -S owlry-lua     # For Lua plugins");
        println!("  yay -S owlry-rune    # For Rune plugins");
    }

    Ok(())
}

/// Run a plugin command
fn cmd_run_plugin_command(plugin_id: &str, command: &str, args: &[String]) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;
    let plugin_path = plugins_dir.join(plugin_id);

    if !plugin_path.exists() {
        return Err(format!("Plugin '{}' not found", plugin_id));
    }

    let manifest_path = plugin_path.join("plugin.toml");
    if !manifest_path.exists() {
        return Err(format!("Plugin '{}' has no manifest", plugin_id));
    }

    let manifest_content = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: PluginManifest = toml::from_str(&manifest_content)
        .map_err(|e| format!("Failed to parse manifest: {}", e))?;

    // Check if plugin provides this command
    let cmd_info = manifest.provides.commands.iter().find(|c| c.name == command);
    if cmd_info.is_none() {
        let available: Vec<_> = manifest.provides.commands.iter().map(|c| c.name.as_str()).collect();
        if available.is_empty() {
            return Err(format!("Plugin '{}' does not provide any CLI commands", plugin_id));
        }
        return Err(format!(
            "Plugin '{}' does not have command '{}'. Available: {}",
            plugin_id, command, available.join(", ")
        ));
    }

    // Check runtime availability
    let runtime = detect_runtime(&manifest);
    check_runtime_available(runtime)?;

    // Execute the command via the plugin runtime
    // The runtime will call the plugin's command handler
    execute_plugin_command(&plugin_path, &manifest, command, args)
}

/// Execute a plugin command through the runtime
fn execute_plugin_command(
    plugin_path: &Path,
    manifest: &PluginManifest,
    command: &str,
    args: &[String],
) -> CommandResult {
    use crate::plugins::runtime_loader::{LoadedRuntime, SYSTEM_RUNTIMES_DIR};

    let runtime = detect_runtime(manifest);

    // Load the appropriate runtime
    let loaded_runtime = match runtime {
        PluginRuntime::Lua => {
            LoadedRuntime::load_lua(plugin_path.parent().unwrap_or(plugin_path))
                .map_err(|e| format!("Failed to load Lua runtime: {}", e))?
        }
        PluginRuntime::Rune => {
            LoadedRuntime::load_rune(plugin_path.parent().unwrap_or(plugin_path))
                .map_err(|e| format!("Failed to load Rune runtime: {}", e))?
        }
    };

    // Build the command query string
    // Format: !CMD:<command>:<arg1>:<arg2>:...
    let mut query_parts = vec!["!CMD".to_string(), command.to_string()];
    query_parts.extend(args.iter().cloned());
    let _query = query_parts.join(":");

    // Find the provider from this plugin and send the command query
    let _provider_name = manifest.provides.providers.first()
        .ok_or_else(|| format!("Plugin '{}' has no providers", manifest.plugin.id))?;

    // Query the provider with the command
    // The runtime will interpret !CMD: prefix as a command invocation
    let _providers = loaded_runtime.providers();

    // For now, we use a simpler approach: invoke the entry point with command args
    // This requires runtime support for command execution
    println!("Executing: owlry plugin run {} {} {}", manifest.plugin.id, command, args.join(" "));
    println!();
    println!("Note: Plugin command execution requires runtime support.");
    println!("The plugin entry point should handle CLI commands via owlry.command.register()");
    println!();
    println!("Runtime: {} ({})", runtime, if PathBuf::from(SYSTEM_RUNTIMES_DIR).join(
        match runtime { PluginRuntime::Lua => "liblua.so", PluginRuntime::Rune => "librune.so" }
    ).exists() { "available" } else { "NOT INSTALLED" });

    // TODO: Implement actual command execution through runtime
    // This would involve:
    // 1. Loading the plugin in the runtime
    // 2. Calling the registered command handler
    // 3. Capturing and displaying output

    Ok(())
}

/// List commands provided by plugins
fn cmd_list_commands(plugin_id: Option<&str>) -> CommandResult {
    let plugins_dir = paths::plugins_dir().ok_or("Could not determine plugins directory")?;

    if !plugins_dir.exists() {
        println!("No plugins installed.");
        return Ok(());
    }

    let discovered = discover_plugins(&plugins_dir).map_err(|e| e.to_string())?;

    if let Some(id) = plugin_id {
        // Show commands for a specific plugin
        let (manifest, _path) = discovered.get(id)
            .ok_or_else(|| format!("Plugin '{}' not found", id))?;

        if manifest.provides.commands.is_empty() {
            println!("Plugin '{}' does not provide any CLI commands.", id);
            return Ok(());
        }

        println!("Commands provided by '{}':\n", id);
        for cmd in &manifest.provides.commands {
            let usage = if cmd.usage.is_empty() {
                String::new()
            } else {
                format!(" {}", cmd.usage)
            };
            println!("  owlry plugin run {} {}{}", id, cmd.name, usage);
            if !cmd.description.is_empty() {
                println!("      {}", cmd.description);
            }
        }
    } else {
        // Show all plugin commands
        let mut found_any = false;

        for (id, (manifest, _path)) in &discovered {
            if manifest.provides.commands.is_empty() {
                continue;
            }

            if !found_any {
                println!("Plugin CLI Commands:\n");
                found_any = true;
            }

            let runtime = detect_runtime(manifest);
            let runtime_available = match runtime {
                PluginRuntime::Lua => lua_runtime_available(),
                PluginRuntime::Rune => rune_runtime_available(),
            };
            let runtime_status = if !runtime_available {
                format!(" [{}]", runtime)
            } else {
                String::new()
            };

            println!("  {} v{}{}", id, manifest.plugin.version, runtime_status);
            for cmd in &manifest.provides.commands {
                let usage = if cmd.usage.is_empty() {
                    String::new()
                } else {
                    format!(" {}", cmd.usage)
                };
                println!("    {} {}{}", id, cmd.name, usage);
                if !cmd.description.is_empty() {
                    println!("        {}", cmd.description);
                }
            }
            println!();
        }

        if !found_any {
            println!("No plugins provide CLI commands.");
            println!();
            println!("Plugins can declare commands in plugin.toml:");
            println!();
            println!("  [[provides.commands]]");
            println!("  name = \"sync\"");
            println!("  description = \"Sync data from remote\"");
            println!("  usage = \"[--force]\"");
        }
    }

    Ok(())
}
