//! File Search Plugin for Owlry
//!
//! A dynamic provider that searches for files using `fd` or `locate`.
//!
//! Examples:
//! - `/ config.toml` → Search for files matching "config.toml"
//! - `file bashrc` → Search for files matching "bashrc"
//! - `find readme` → Search for files matching "readme"
//!
//! Dependencies:
//! - fd (preferred) or locate

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};
use std::path::Path;
use std::process::Command;

// Plugin metadata
const PLUGIN_ID: &str = "filesearch";
const PLUGIN_NAME: &str = "File Search";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Find files with fd or locate";

// Provider metadata
const PROVIDER_ID: &str = "filesearch";
const PROVIDER_NAME: &str = "Files";
const PROVIDER_PREFIX: &str = "/";
const PROVIDER_ICON: &str = "folder";
const PROVIDER_TYPE_ID: &str = "filesearch";

// Maximum results to return
const MAX_RESULTS: usize = 20;

#[derive(Debug, Clone, Copy)]
enum SearchTool {
    Fd,
    Locate,
    None,
}

/// File search provider state
struct FileSearchState {
    search_tool: SearchTool,
    home: String,
}

impl FileSearchState {
    fn new() -> Self {
        let search_tool = Self::detect_search_tool();
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        Self { search_tool, home }
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

    /// Extract the search term from the query
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();

        if let Some(rest) = trimmed.strip_prefix("/ ") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix("/") {
            Some(rest.trim())
        } else {
            // Handle "file " and "find " prefixes (case-insensitive), or raw query in filter mode
            let lower = trimmed.to_lowercase();
            if lower.starts_with("file ") || lower.starts_with("find ") {
                Some(trimmed[5..].trim())
            } else {
                Some(trimmed)
            }
        }
    }

    /// Evaluate a query and return file results
    fn evaluate(&self, query: &str) -> Vec<PluginItem> {
        let search_term = match Self::extract_search_term(query) {
            Some(t) if !t.is_empty() => t,
            _ => return Vec::new(),
        };

        self.search_files(search_term)
    }

    fn search_files(&self, pattern: &str) -> Vec<PluginItem> {
        match self.search_tool {
            SearchTool::Fd => self.search_with_fd(pattern),
            SearchTool::Locate => self.search_with_locate(pattern),
            SearchTool::None => Vec::new(),
        }
    }

    fn search_with_fd(&self, pattern: &str) -> Vec<PluginItem> {
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

    fn search_with_locate(&self, pattern: &str) -> Vec<PluginItem> {
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

    fn parse_file_results(&self, output: &str) -> Vec<PluginItem> {
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

                // Get filename for display
                let filename = Path::new(&full_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| full_path.clone());

                // Determine icon based on whether it's a directory
                let is_dir = Path::new(&full_path).is_dir();
                let icon = if is_dir { "folder" } else { "text-x-generic" };

                // Command to open with xdg-open
                let command = format!("xdg-open '{}'", full_path.replace('\'', "'\\''"));

                PluginItem::new(format!("file:{}", full_path), filename, command)
                    .with_description(full_path.clone())
                    .with_icon(icon)
                    .with_keywords(vec!["file".to_string()])
            })
            .collect()
    }
}

// ============================================================================
// Plugin Interface Implementation
// ============================================================================

extern "C" fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: RString::from(PLUGIN_ID),
        name: RString::from(PLUGIN_NAME),
        version: RString::from(PLUGIN_VERSION),
        description: RString::from(PLUGIN_DESCRIPTION),
        api_version: API_VERSION,
    }
}

extern "C" fn plugin_providers() -> RVec<ProviderInfo> {
    vec![ProviderInfo {
        id: RString::from(PROVIDER_ID),
        name: RString::from(PROVIDER_NAME),
        prefix: ROption::RSome(RString::from(PROVIDER_PREFIX)),
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Dynamic,
        type_id: RString::from(PROVIDER_TYPE_ID),
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(FileSearchState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(_handle: ProviderHandle) -> RVec<PluginItem> {
    // Dynamic provider - refresh does nothing
    RVec::new()
}

extern "C" fn provider_query(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<FileSearchState>
    let state = unsafe { &*(handle.ptr as *const FileSearchState) };

    let query_str = query.as_str();

    state.evaluate(query_str).into()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<FileSearchState>
        unsafe {
            handle.drop_as::<FileSearchState>();
        }
    }
}

// Register the plugin vtable
owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_search_term() {
        assert_eq!(
            FileSearchState::extract_search_term("/ config.toml"),
            Some("config.toml")
        );
        assert_eq!(
            FileSearchState::extract_search_term("/config"),
            Some("config")
        );
        assert_eq!(
            FileSearchState::extract_search_term("file bashrc"),
            Some("bashrc")
        );
        assert_eq!(
            FileSearchState::extract_search_term("find readme"),
            Some("readme")
        );
    }

    #[test]
    fn test_extract_search_term_empty() {
        assert_eq!(FileSearchState::extract_search_term("/"), Some(""));
        assert_eq!(FileSearchState::extract_search_term("/ "), Some(""));
    }

    #[test]
    fn test_command_exists() {
        // 'which' should exist on any Unix system
        assert!(FileSearchState::command_exists("which"));
        // This should not exist
        assert!(!FileSearchState::command_exists("nonexistent-command-12345"));
    }

    #[test]
    fn test_detect_search_tool() {
        // Just ensure it doesn't panic
        let _ = FileSearchState::detect_search_tool();
    }

    #[test]
    fn test_state_new() {
        let state = FileSearchState::new();
        assert!(!state.home.is_empty());
    }

    #[test]
    fn test_evaluate_empty() {
        let state = FileSearchState::new();
        let results = state.evaluate("/");
        assert!(results.is_empty());

        let results = state.evaluate("/ ");
        assert!(results.is_empty());
    }
}
