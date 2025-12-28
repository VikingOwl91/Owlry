use crate::providers::{LaunchItem, ProviderType};
use log::{debug, warn};
use std::process::Command;

/// File search provider - uses fd or locate for fast file finding
pub struct FileSearchProvider {
    search_tool: SearchTool,
    max_results: usize,
}

#[derive(Debug, Clone, Copy)]
enum SearchTool {
    Fd,
    Locate,
    None,
}

impl FileSearchProvider {
    pub fn new() -> Self {
        let search_tool = Self::detect_search_tool();
        debug!("File search using: {:?}", search_tool);

        Self {
            search_tool,
            max_results: 20,
        }
    }

    fn detect_search_tool() -> SearchTool {
        // Prefer fd (faster, respects .gitignore)
        if Self::command_exists("fd") {
            return SearchTool::Fd;
        }
        // Fall back to locate (requires updatedb)
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

    /// Check if query is a file search query
    /// Triggers on: `/ query`, `file query`, `find query`
    pub fn is_file_query(query: &str) -> bool {
        let trimmed = query.trim();
        trimmed.starts_with("/ ")
            || trimmed.starts_with("/")
            || trimmed.to_lowercase().starts_with("file ")
            || trimmed.to_lowercase().starts_with("find ")
    }

    /// Extract the search term from the query
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();

        if let Some(rest) = trimmed.strip_prefix("/ ") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix("/") {
            Some(rest.trim())
        } else if trimmed.to_lowercase().starts_with("file ") {
            Some(trimmed[5..].trim())
        } else if trimmed.to_lowercase().starts_with("find ") {
            Some(trimmed[5..].trim())
        } else {
            None
        }
    }

    /// Evaluate a file search query
    pub fn evaluate(&self, query: &str) -> Vec<LaunchItem> {
        let search_term = match Self::extract_search_term(query) {
            Some(t) if !t.is_empty() => t,
            _ => return Vec::new(),
        };

        self.search_files(search_term)
    }

    /// Evaluate a raw search term (for :file filter mode)
    pub fn evaluate_raw(&self, search_term: &str) -> Vec<LaunchItem> {
        let trimmed = search_term.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        self.search_files(trimmed)
    }

    fn search_files(&self, pattern: &str) -> Vec<LaunchItem> {
        match self.search_tool {
            SearchTool::Fd => self.search_with_fd(pattern),
            SearchTool::Locate => self.search_with_locate(pattern),
            SearchTool::None => {
                debug!("No file search tool available");
                Vec::new()
            }
        }
    }

    fn search_with_fd(&self, pattern: &str) -> Vec<LaunchItem> {
        // fd searches from home directory by default
        let home = dirs::home_dir().unwrap_or_default();

        let output = match Command::new("fd")
            .args([
                "--max-results",
                &self.max_results.to_string(),
                "--type",
                "f", // Files only
                "--type",
                "d", // And directories
                pattern,
            ])
            .current_dir(&home)
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("Failed to run fd: {}", e);
                return Vec::new();
            }
        };

        self.parse_file_results(&String::from_utf8_lossy(&output.stdout), &home)
    }

    fn search_with_locate(&self, pattern: &str) -> Vec<LaunchItem> {
        let home = dirs::home_dir().unwrap_or_default();

        let output = match Command::new("locate")
            .args([
                "--limit",
                &self.max_results.to_string(),
                "--ignore-case",
                pattern,
            ])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("Failed to run locate: {}", e);
                return Vec::new();
            }
        };

        self.parse_file_results(&String::from_utf8_lossy(&output.stdout), &home)
    }

    fn parse_file_results(&self, output: &str, home: &std::path::Path) -> Vec<LaunchItem> {
        output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|path| {
                let path = path.trim();
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else {
                    home.join(path).to_string_lossy().to_string()
                };

                // Get filename for display
                let filename = std::path::Path::new(&full_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| full_path.clone());

                // Determine icon based on whether it's a directory
                let is_dir = std::path::Path::new(&full_path).is_dir();
                let icon = if is_dir {
                    "folder"
                } else {
                    "text-x-generic"
                };

                // Command to open with xdg-open
                let command = format!("xdg-open '{}'", full_path.replace('\'', "'\\''"));

                LaunchItem {
                    id: format!("file:{}", full_path),
                    name: filename,
                    description: Some(full_path.clone()),
                    icon: Some(icon.to_string()),
                    provider: ProviderType::Files,
                    command,
                    terminal: false,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_file_query() {
        assert!(FileSearchProvider::is_file_query("/ config"));
        assert!(FileSearchProvider::is_file_query("/config"));
        assert!(FileSearchProvider::is_file_query("file config"));
        assert!(FileSearchProvider::is_file_query("find config"));
        assert!(!FileSearchProvider::is_file_query("config"));
        assert!(!FileSearchProvider::is_file_query("? search"));
    }

    #[test]
    fn test_extract_search_term() {
        assert_eq!(
            FileSearchProvider::extract_search_term("/ config.toml"),
            Some("config.toml")
        );
        assert_eq!(
            FileSearchProvider::extract_search_term("file bashrc"),
            Some("bashrc")
        );
    }
}
