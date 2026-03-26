//! Plugin manifest (plugin.toml) parsing

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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

impl PluginManifest {
    /// Load a plugin manifest from a plugin.toml file
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read manifest: {}", e))?;
        let manifest: PluginManifest =
            toml::from_str(&content).map_err(|e| format!("Failed to parse manifest: {}", e))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest
    fn validate(&self) -> Result<(), String> {
        // Validate plugin ID format
        if self.plugin.id.is_empty() {
            return Err("Plugin ID cannot be empty".to_string());
        }

        if !self
            .plugin
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err("Plugin ID must be lowercase alphanumeric with hyphens".to_string());
        }

        // Validate version format
        if semver::Version::parse(&self.plugin.version).is_err() {
            return Err(format!("Invalid version format: {}", self.plugin.version));
        }

        // Validate owlry_version constraint
        if semver::VersionReq::parse(&self.plugin.owlry_version).is_err() {
            return Err(format!(
                "Invalid owlry_version constraint: {}",
                self.plugin.owlry_version
            ));
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
