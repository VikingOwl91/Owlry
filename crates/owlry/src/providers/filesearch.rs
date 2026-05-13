//! File search provider.
//!
//! Dynamic provider that searches for files using `fd` (preferred) or
//! `locate`. Triggered by:
//! - `/ name` / `/name` (slash prefix)
//! - `file name` / `find name` (word prefix)
//!
//! External dependencies:
//! - `fd` (preferred) or `locate`

use super::{DynamicProvider, ItemSource, LaunchItem, ProviderType};
use std::path::Path;
use std::process::Command;

const TYPE_ID: &str = "filesearch";
const MAX_RESULTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchTool {
    Fd,
    Locate,
    None,
}

/// Dynamic file search provider — shells out to `fd` or `locate` per keystroke.
pub(crate) struct FileSearchProvider {
    search_tool: SearchTool,
    // TODO(v2.x): plumb via constructor (search roots, extra flags).
    home: String,
}

impl Default for FileSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSearchProvider {
    pub fn new() -> Self {
        let search_tool = Self::detect_search_tool();
        // TODO(v2.x): plumb via constructor.
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        Self { search_tool, home }
    }

    fn detect_search_tool() -> SearchTool {
        // Prefer fd (faster, respects .gitignore).
        if Self::command_exists("fd") {
            return SearchTool::Fd;
        }
        // Fall back to locate (requires updatedb).
        if Self::command_exists("locate") {
            return SearchTool::Locate;
        }
        SearchTool::None
    }

    fn command_exists(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Extract the search term from the query.
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();

        if let Some(rest) = trimmed.strip_prefix("/ ") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix('/') {
            Some(rest.trim())
        } else {
            // Handle "file " and "find " prefixes (case-insensitive), or raw
            // query in filter mode.
            let lower = trimmed.to_lowercase();
            if lower.starts_with("file ") || lower.starts_with("find ") {
                Some(trimmed[5..].trim())
            } else {
                Some(trimmed)
            }
        }
    }

    fn evaluate(&self, query: &str) -> Vec<LaunchItem> {
        let search_term = match Self::extract_search_term(query) {
            Some(t) if !t.is_empty() => t,
            _ => return Vec::new(),
        };

        self.search_files(search_term)
    }

    fn search_files(&self, pattern: &str) -> Vec<LaunchItem> {
        match self.search_tool {
            SearchTool::Fd => self.search_with_fd(pattern),
            SearchTool::Locate => self.search_with_locate(pattern),
            SearchTool::None => Vec::new(),
        }
    }

    fn search_with_fd(&self, pattern: &str) -> Vec<LaunchItem> {
        let output = match Command::new("fd")
            .args([
                "--max-results",
                &MAX_RESULTS.to_string(),
                "--type",
                "f", // Files only
                "--type",
                "d", // And directories
                pattern,
            ])
            .current_dir(&self.home)
            .output()
        {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        self.parse_file_results(&String::from_utf8_lossy(&output.stdout))
    }

    fn search_with_locate(&self, pattern: &str) -> Vec<LaunchItem> {
        let output = match Command::new("locate")
            .args([
                "--limit",
                &MAX_RESULTS.to_string(),
                "--ignore-case",
                pattern,
            ])
            .output()
        {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        self.parse_file_results(&String::from_utf8_lossy(&output.stdout))
    }

    fn parse_file_results(&self, output: &str) -> Vec<LaunchItem> {
        output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|path| {
                let path = path.trim();
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("{}/{}", self.home, path)
                };

                let filename = Path::new(&full_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| full_path.clone());

                let is_dir = Path::new(&full_path).is_dir();
                let icon = if is_dir { "folder" } else { "text-x-generic" };

                let command = format!("xdg-open '{}'", full_path.replace('\'', "'\\''"));

                LaunchItem {
                    id: format!("file:{}", full_path),
                    name: filename,
                    description: Some(full_path.clone()),
                    icon: Some(icon.to_string()),
                    provider: ProviderType::Plugin(TYPE_ID.into()),
                    command,
                    terminal: false,
                    tags: vec!["file".to_string()],
                    source: ItemSource::Core,
                }
            })
            .collect()
    }
}

impl DynamicProvider for FileSearchProvider {
    fn name(&self) -> &str {
        "Files"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn priority(&self) -> u32 {
        8_000
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        self.evaluate(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_search_term() {
        assert_eq!(
            FileSearchProvider::extract_search_term("/ config.toml"),
            Some("config.toml")
        );
        assert_eq!(
            FileSearchProvider::extract_search_term("/config"),
            Some("config")
        );
        assert_eq!(
            FileSearchProvider::extract_search_term("file bashrc"),
            Some("bashrc")
        );
        assert_eq!(
            FileSearchProvider::extract_search_term("find readme"),
            Some("readme")
        );
    }

    #[test]
    fn test_extract_search_term_empty() {
        assert_eq!(FileSearchProvider::extract_search_term("/"), Some(""));
        assert_eq!(FileSearchProvider::extract_search_term("/ "), Some(""));
    }

    #[test]
    fn test_command_exists() {
        // 'which' should exist on any Unix system.
        assert!(FileSearchProvider::command_exists("which"));
        // This should not exist.
        assert!(!FileSearchProvider::command_exists(
            "nonexistent-command-12345"
        ));
    }

    #[test]
    fn test_detect_search_tool() {
        // Just ensure it doesn't panic.
        let _ = FileSearchProvider::detect_search_tool();
    }

    #[test]
    fn test_state_new() {
        let provider = FileSearchProvider::new();
        assert!(!provider.home.is_empty());
    }

    #[test]
    fn test_evaluate_empty() {
        let provider = FileSearchProvider::new();
        let results = provider.evaluate("/");
        assert!(results.is_empty());

        let results = provider.evaluate("/ ");
        assert!(results.is_empty());
    }

    #[test]
    fn provider_type_is_filesearch_plugin() {
        let p = FileSearchProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("filesearch".into()));
    }
}
