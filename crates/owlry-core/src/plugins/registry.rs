//! Plugin registry client for discovering and installing remote plugins
//!
//! The registry is a git repository containing an `index.toml` file with
//! plugin metadata. Plugins are installed by cloning their source repositories.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::paths;

/// Default registry URL (can be overridden in config)
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/owlry/plugin-registry/main/index.toml";

/// Cache duration for registry index (1 hour)
const CACHE_DURATION: Duration = Duration::from_secs(3600);

/// Registry index containing all available plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    /// Registry metadata
    #[serde(default)]
    pub registry: RegistryMeta,
    /// Available plugins
    #[serde(default)]
    pub plugins: Vec<RegistryPlugin>,
}

/// Registry metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryMeta {
    /// Registry name
    #[serde(default)]
    pub name: String,
    /// Registry description
    #[serde(default)]
    pub description: String,
    /// Registry maintainer URL
    #[serde(default)]
    pub url: String,
}

/// Plugin entry in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPlugin {
    /// Unique plugin identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Latest version
    pub version: String,
    /// Short description
    #[serde(default)]
    pub description: String,
    /// Plugin author
    #[serde(default)]
    pub author: String,
    /// Git repository URL for installation
    pub repository: String,
    /// Search tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Minimum owlry version required
    #[serde(default)]
    pub owlry_version: String,
    /// License identifier
    #[serde(default)]
    pub license: String,
}

/// Registry client for fetching and searching plugins
pub struct RegistryClient {
    /// Registry URL (index.toml location)
    registry_url: String,
    /// Local cache directory
    cache_dir: PathBuf,
}

impl RegistryClient {
    /// Create a new registry client with the given URL
    pub fn new(registry_url: &str) -> Self {
        let cache_dir = paths::owlry_cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp/owlry"))
            .join("registry");

        Self {
            registry_url: registry_url.to_string(),
            cache_dir,
        }
    }

    /// Create a client with the default registry URL
    pub fn default_registry() -> Self {
        Self::new(DEFAULT_REGISTRY_URL)
    }

    /// Get the path to the cached index file
    fn cache_path(&self) -> PathBuf {
        self.cache_dir.join("index.toml")
    }

    /// Check if the cache is valid (exists and not expired)
    fn is_cache_valid(&self) -> bool {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            return false;
        }

        if let Ok(metadata) = fs::metadata(&cache_path)
            && let Ok(modified) = metadata.modified()
            && let Ok(elapsed) = SystemTime::now().duration_since(modified)
        {
            return elapsed < CACHE_DURATION;
        }

        false
    }

    /// Fetch the registry index (from cache or network)
    pub fn fetch_index(&self, force_refresh: bool) -> Result<RegistryIndex, String> {
        // Use cache if valid and not forcing refresh
        if !force_refresh
            && self.is_cache_valid()
            && let Ok(content) = fs::read_to_string(self.cache_path())
            && let Ok(index) = toml::from_str(&content)
        {
            return Ok(index);
        }

        // Fetch from network
        self.fetch_from_network()
    }

    /// Fetch the index from the network and cache it
    fn fetch_from_network(&self) -> Result<RegistryIndex, String> {
        // Use curl for fetching (available on most systems)
        let output = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "30", &self.registry_url])
            .output()
            .map_err(|e| format!("Failed to run curl: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to fetch registry: {}", stderr.trim()));
        }

        let content = String::from_utf8_lossy(&output.stdout);

        // Parse the index
        let index: RegistryIndex = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse registry index: {}", e))?;

        // Cache the result
        if let Err(e) = self.cache_index(&content) {
            eprintln!("Warning: Failed to cache registry index: {}", e);
        }

        Ok(index)
    }

    /// Cache the index content to disk
    fn cache_index(&self, content: &str) -> Result<(), String> {
        fs::create_dir_all(&self.cache_dir)
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;

        fs::write(self.cache_path(), content)
            .map_err(|e| format!("Failed to write cache file: {}", e))?;

        Ok(())
    }

    /// Search for plugins matching a query
    pub fn search(&self, query: &str, force_refresh: bool) -> Result<Vec<RegistryPlugin>, String> {
        let index = self.fetch_index(force_refresh)?;
        let query_lower = query.to_lowercase();

        let matches: Vec<_> = index
            .plugins
            .into_iter()
            .filter(|p| {
                p.id.to_lowercase().contains(&query_lower)
                    || p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
                    || p.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect();

        Ok(matches)
    }

    /// Find a specific plugin by ID
    pub fn find(&self, id: &str, force_refresh: bool) -> Result<Option<RegistryPlugin>, String> {
        let index = self.fetch_index(force_refresh)?;

        Ok(index.plugins.into_iter().find(|p| p.id == id))
    }

    /// List all available plugins
    pub fn list_all(&self, force_refresh: bool) -> Result<Vec<RegistryPlugin>, String> {
        let index = self.fetch_index(force_refresh)?;
        Ok(index.plugins)
    }

    /// Clear the cache
    #[allow(dead_code)]
    pub fn clear_cache(&self) -> Result<(), String> {
        let cache_path = self.cache_path();
        if cache_path.exists() {
            fs::remove_file(&cache_path).map_err(|e| format!("Failed to remove cache: {}", e))?;
        }
        Ok(())
    }

    /// Get the repository URL for a plugin
    #[allow(dead_code)]
    pub fn get_install_url(&self, id: &str) -> Result<String, String> {
        match self.find(id, false)? {
            Some(plugin) => Ok(plugin.repository),
            None => Err(format!("Plugin '{}' not found in registry", id)),
        }
    }
}

/// Check if a string looks like a URL (for distinguishing registry names from URLs)
pub fn is_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("git://")
}

/// Check if a string looks like a local path
pub fn is_path(s: &str) -> bool {
    s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with('~')
        || Path::new(s).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_registry_index() {
        let toml_str = r#"
[registry]
name = "Test Registry"
description = "A test registry"

[[plugins]]
id = "test-plugin"
name = "Test Plugin"
version = "1.0.0"
description = "A test plugin"
author = "Test Author"
repository = "https://github.com/test/plugin"
tags = ["test", "example"]
owlry_version = ">=0.3.0"
"#;

        let index: RegistryIndex = toml::from_str(toml_str).unwrap();
        assert_eq!(index.registry.name, "Test Registry");
        assert_eq!(index.plugins.len(), 1);
        assert_eq!(index.plugins[0].id, "test-plugin");
        assert_eq!(index.plugins[0].tags, vec!["test", "example"]);
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("https://github.com/user/repo"));
        assert!(is_url("http://example.com"));
        assert!(is_url("git@github.com:user/repo.git"));
        assert!(!is_url("my-plugin"));
        assert!(!is_url("/path/to/plugin"));
    }

    #[test]
    fn test_is_path() {
        assert!(is_path("/absolute/path"));
        assert!(is_path("./relative/path"));
        assert!(is_path("../parent/path"));
        assert!(is_path("~/home/path"));
        assert!(!is_path("my-plugin"));
        assert!(!is_path("https://example.com"));
    }
}
