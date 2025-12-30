//! Plugin manifest (plugin.toml) parsing

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::error::{PluginError, PluginResult};

/// Plugin manifest loaded from plugin.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    #[serde(default)]
    pub provides: PluginProvides,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub settings: HashMap<String, toml::Value>,
}

/// Core plugin information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique plugin identifier (lowercase, alphanumeric, hyphens)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Semantic version
    pub version: String,
    /// Short description
    #[serde(default)]
    pub description: String,
    /// Plugin author
    #[serde(default)]
    pub author: String,
    /// License identifier
    #[serde(default)]
    pub license: String,
    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,
    /// Required owlry version (semver constraint)
    #[serde(default = "default_owlry_version")]
    pub owlry_version: String,
    /// Entry point file (relative to plugin directory)
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_owlry_version() -> String {
    ">=0.1.0".to_string()
}

fn default_entry() -> String {
    "init.lua".to_string()
}

/// What the plugin provides
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginProvides {
    /// Provider names this plugin registers
    #[serde(default)]
    pub providers: Vec<String>,
    /// Whether this plugin registers actions
    #[serde(default)]
    pub actions: bool,
    /// Theme names this plugin contributes
    #[serde(default)]
    pub themes: Vec<String>,
    /// Whether this plugin registers hooks
    #[serde(default)]
    pub hooks: bool,
    /// CLI commands this plugin provides
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
}

/// A CLI command provided by a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    /// Command name (e.g., "add", "list", "sync")
    pub name: String,
    /// Short description shown in help
    #[serde(default)]
    pub description: String,
    /// Usage pattern (e.g., "<url> [name]")
    #[serde(default)]
    pub usage: String,
}

/// Plugin permissions/capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPermissions {
    /// Allow network/HTTP requests
    #[serde(default)]
    pub network: bool,
    /// Filesystem paths the plugin can access (beyond its own directory)
    #[serde(default)]
    pub filesystem: Vec<String>,
    /// Commands the plugin is allowed to run
    #[serde(default)]
    pub run_commands: Vec<String>,
    /// Environment variables the plugin reads
    #[serde(default)]
    pub environment: Vec<String>,
}

// ============================================================================
// Plugin Discovery (no Lua dependency)
// ============================================================================

/// Discover all plugins in a directory
///
/// Returns a map of plugin ID -> (manifest, path)
pub fn discover_plugins(plugins_dir: &Path) -> PluginResult<HashMap<String, (PluginManifest, PathBuf)>> {
    let mut plugins = HashMap::new();

    if !plugins_dir.exists() {
        log::debug!("Plugins directory does not exist: {}", plugins_dir.display());
        return Ok(plugins);
    }

    let entries = std::fs::read_dir(plugins_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.toml");
        if !manifest_path.exists() {
            log::debug!("Skipping {}: no plugin.toml", path.display());
            continue;
        }

        match PluginManifest::load(&manifest_path) {
            Ok(manifest) => {
                let id = manifest.plugin.id.clone();
                if plugins.contains_key(&id) {
                    log::warn!("Duplicate plugin ID '{}', skipping {}", id, path.display());
                    continue;
                }
                log::info!("Discovered plugin: {} v{}", manifest.plugin.name, manifest.plugin.version);
                plugins.insert(id, (manifest, path));
            }
            Err(e) => {
                log::warn!("Failed to load plugin at {}: {}", path.display(), e);
            }
        }
    }

    Ok(plugins)
}

/// Check if a plugin is compatible with the given owlry version
pub fn check_compatibility(manifest: &PluginManifest, owlry_version: &str) -> PluginResult<()> {
    if !manifest.is_compatible_with(owlry_version) {
        return Err(PluginError::VersionMismatch {
            plugin: manifest.plugin.id.clone(),
            required: manifest.plugin.owlry_version.clone(),
            current: owlry_version.to_string(),
        });
    }
    Ok(())
}

// ============================================================================
// PluginManifest Implementation
// ============================================================================

impl PluginManifest {
    /// Load a plugin manifest from a plugin.toml file
    pub fn load(path: &Path) -> PluginResult<Self> {
        let content = std::fs::read_to_string(path)?;
        let manifest: PluginManifest = toml::from_str(&content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Load from a plugin directory (looks for plugin.toml inside)
    #[allow(dead_code)]
    pub fn load_from_dir(plugin_dir: &Path) -> PluginResult<Self> {
        let manifest_path = plugin_dir.join("plugin.toml");
        if !manifest_path.exists() {
            return Err(PluginError::InvalidManifest {
                plugin: plugin_dir.display().to_string(),
                message: "plugin.toml not found".to_string(),
            });
        }
        Self::load(&manifest_path)
    }

    /// Validate the manifest
    fn validate(&self) -> PluginResult<()> {
        // Validate plugin ID format
        if self.plugin.id.is_empty() {
            return Err(PluginError::InvalidManifest {
                plugin: self.plugin.id.clone(),
                message: "Plugin ID cannot be empty".to_string(),
            });
        }

        if !self.plugin.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(PluginError::InvalidManifest {
                plugin: self.plugin.id.clone(),
                message: "Plugin ID must be lowercase alphanumeric with hyphens".to_string(),
            });
        }

        // Validate version format
        if semver::Version::parse(&self.plugin.version).is_err() {
            return Err(PluginError::InvalidManifest {
                plugin: self.plugin.id.clone(),
                message: format!("Invalid version format: {}", self.plugin.version),
            });
        }

        // Validate owlry_version constraint
        if semver::VersionReq::parse(&self.plugin.owlry_version).is_err() {
            return Err(PluginError::InvalidManifest {
                plugin: self.plugin.id.clone(),
                message: format!("Invalid owlry_version constraint: {}", self.plugin.owlry_version),
            });
        }

        Ok(())
    }

    /// Check if this plugin is compatible with the given owlry version
    pub fn is_compatible_with(&self, owlry_version: &str) -> bool {
        let req = match semver::VersionReq::parse(&self.plugin.owlry_version) {
            Ok(r) => r,
            Err(_) => return false,
        };
        let version = match semver::Version::parse(owlry_version) {
            Ok(v) => v,
            Err(_) => return false,
        };
        req.matches(&version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_manifest() {
        let toml_str = r#"
[plugin]
id = "test-plugin"
name = "Test Plugin"
version = "1.0.0"
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.plugin.id, "test-plugin");
        assert_eq!(manifest.plugin.name, "Test Plugin");
        assert_eq!(manifest.plugin.version, "1.0.0");
        assert_eq!(manifest.plugin.entry, "init.lua");
    }

    #[test]
    fn test_parse_full_manifest() {
        let toml_str = r#"
[plugin]
id = "my-provider"
name = "My Provider"
version = "1.2.3"
description = "A test provider"
author = "Test Author"
license = "MIT"
owlry_version = ">=0.4.0"
entry = "main.lua"

[provides]
providers = ["my-provider"]
actions = true
themes = ["dark"]
hooks = true

[permissions]
network = true
filesystem = ["~/.config/myapp"]
run_commands = ["myapp"]
environment = ["MY_API_KEY"]

[settings]
max_results = 20
api_url = "https://api.example.com"
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.plugin.id, "my-provider");
        assert!(manifest.provides.actions);
        assert!(manifest.permissions.network);
        assert_eq!(manifest.permissions.run_commands, vec!["myapp"]);
    }

    #[test]
    fn test_version_compatibility() {
        let toml_str = r#"
[plugin]
id = "test"
name = "Test"
version = "1.0.0"
owlry_version = ">=0.3.0, <1.0.0"
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.is_compatible_with("0.3.5"));
        assert!(manifest.is_compatible_with("0.4.0"));
        assert!(!manifest.is_compatible_with("0.2.0"));
        assert!(!manifest.is_compatible_with("1.0.0"));
    }
}
