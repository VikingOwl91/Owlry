//! Plugin manifest parsing for Rune plugins

use serde::Deserialize;
use std::path::Path;

/// Plugin manifest from plugin.toml
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    #[serde(default)]
    pub provides: PluginProvides,
    #[serde(default)]
    pub permissions: PluginPermissions,
    /// Provider declarations from [[providers]] sections
    #[serde(default)]
    pub providers: Vec<ProviderDecl>,
}

/// A provider declared in [[providers]] section of plugin.toml
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderDecl {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_provider_type", rename = "type")]
    pub provider_type: String,
    #[serde(default)]
    pub type_id: Option<String>,
    #[serde(default)]
    pub tab_label: Option<String>,
    #[serde(default)]
    pub search_noun: Option<String>,
}

fn default_provider_type() -> String {
    "static".to_string()
}

/// Core plugin information
#[derive(Debug, Clone, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_owlry_version")]
    pub owlry_version: String,
    #[serde(default = "default_entry", alias = "entry_point")]
    pub entry: String,
}

fn default_owlry_version() -> String {
    ">=0.1.0".to_string()
}

fn default_entry() -> String {
    "main.rn".to_string()
}

/// What the plugin provides
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PluginProvides {
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub actions: bool,
    #[serde(default)]
    pub themes: Vec<String>,
    #[serde(default)]
    pub hooks: bool,
}

/// Plugin permissions
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PluginPermissions {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem: Vec<String>,
    #[serde(default)]
    pub run_commands: Vec<String>,
}

impl PluginManifest {
    /// Load manifest from a plugin.toml file
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

        // Rune plugins must have .rn entry point
        if !self.plugin.entry.ends_with(".rn") {
            return Err("Entry point must be a .rn file for Rune plugins".to_string());
        }

        Ok(())
    }

    /// Check compatibility with owlry version
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
        assert_eq!(manifest.plugin.entry, "main.rn");
    }

    #[test]
    fn test_validate_entry_point() {
        let toml_str = r#"
[plugin]
id = "test"
name = "Test"
version = "1.0.0"
entry = "main.lua"
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.validate().is_err()); // .lua not allowed for Rune
    }

    #[test]
    fn test_version_compatibility() {
        let toml_str = r#"
[plugin]
id = "test"
name = "Test"
version = "1.0.0"
owlry_version = ">=0.3.0"
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.is_compatible_with("0.3.5"));
        assert!(!manifest.is_compatible_with("0.2.0"));
    }
}
