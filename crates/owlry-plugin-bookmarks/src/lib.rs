//! Bookmarks Plugin for Owlry
//!
//! A static provider that reads browser bookmarks from Chrome/Chromium.
//! Firefox support would require the rusqlite crate for reading places.sqlite.
//!
//! Supported browsers:
//! - Chrome
//! - Chromium
//! - Brave
//! - Edge

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

// Plugin metadata
const PLUGIN_ID: &str = "bookmarks";
const PLUGIN_NAME: &str = "Bookmarks";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Browser bookmark search";

// Provider metadata
const PROVIDER_ID: &str = "bookmarks";
const PROVIDER_NAME: &str = "Bookmarks";
const PROVIDER_PREFIX: &str = ":bm";
const PROVIDER_ICON: &str = "web-browser";
const PROVIDER_TYPE_ID: &str = "bookmarks";

/// Bookmarks provider state - holds cached items
struct BookmarksState {
    items: Vec<PluginItem>,
}

impl BookmarksState {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn chromium_bookmark_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(config_dir) = dirs::config_dir() {
            // Chrome
            paths.push(config_dir.join("google-chrome/Default/Bookmarks"));
            paths.push(config_dir.join("google-chrome-stable/Default/Bookmarks"));

            // Chromium
            paths.push(config_dir.join("chromium/Default/Bookmarks"));

            // Brave
            paths.push(config_dir.join("BraveSoftware/Brave-Browser/Default/Bookmarks"));

            // Edge
            paths.push(config_dir.join("microsoft-edge/Default/Bookmarks"));
        }

        paths
    }

    fn load_bookmarks(&mut self) {
        self.items.clear();

        // Load Chrome/Chromium bookmarks
        for path in Self::chromium_bookmark_paths() {
            if path.exists() {
                self.read_chrome_bookmarks(&path);
            }
        }
    }

    fn read_chrome_bookmarks(&mut self, path: &PathBuf) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let bookmarks: ChromeBookmarks = match serde_json::from_str(&content) {
            Ok(b) => b,
            Err(_) => return,
        };

        // Process bookmark bar and other folders
        if let Some(roots) = bookmarks.roots {
            if let Some(bar) = roots.bookmark_bar {
                self.process_chrome_folder(&bar);
            }
            if let Some(other) = roots.other {
                self.process_chrome_folder(&other);
            }
            if let Some(synced) = roots.synced {
                self.process_chrome_folder(&synced);
            }
        }
    }

    fn process_chrome_folder(&mut self, folder: &ChromeBookmarkNode) {
        if let Some(ref children) = folder.children {
            for child in children {
                match child.node_type.as_deref() {
                    Some("url") => {
                        if let Some(ref url) = child.url {
                            let name = child.name.clone().unwrap_or_else(|| url.clone());

                            self.items.push(
                                PluginItem::new(
                                    format!("bookmark:{}", url),
                                    name,
                                    format!("xdg-open '{}'", url.replace('\'', "'\\''")),
                                )
                                .with_description(url.clone())
                                .with_icon(PROVIDER_ICON)
                                .with_keywords(vec!["bookmark".to_string(), "web".to_string()]),
                            );
                        }
                    }
                    Some("folder") => {
                        // Recursively process subfolders
                        self.process_chrome_folder(child);
                    }
                    _ => {}
                }
            }
        }
    }
}

// Chrome bookmark JSON structures
#[derive(Debug, Deserialize)]
struct ChromeBookmarks {
    roots: Option<ChromeBookmarkRoots>,
}

#[derive(Debug, Deserialize)]
struct ChromeBookmarkRoots {
    bookmark_bar: Option<ChromeBookmarkNode>,
    other: Option<ChromeBookmarkNode>,
    synced: Option<ChromeBookmarkNode>,
}

#[derive(Debug, Deserialize)]
struct ChromeBookmarkNode {
    name: Option<String>,
    url: Option<String>,
    #[serde(rename = "type")]
    node_type: Option<String>,
    children: Option<Vec<ChromeBookmarkNode>>,
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
        provider_type: ProviderKind::Static,
        type_id: RString::from(PROVIDER_TYPE_ID),
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(BookmarksState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<BookmarksState>
    let state = unsafe { &mut *(handle.ptr as *mut BookmarksState) };

    // Load bookmarks
    state.load_bookmarks();

    // Return items
    state.items.to_vec().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query is handled by the core using cached items
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<BookmarksState>
        unsafe {
            handle.drop_as::<BookmarksState>();
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
    fn test_bookmarks_state_new() {
        let state = BookmarksState::new();
        assert!(state.items.is_empty());
    }

    #[test]
    fn test_chromium_paths() {
        let paths = BookmarksState::chromium_bookmark_paths();
        // Should have at least some paths configured
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_parse_chrome_bookmarks() {
        let json = r#"{
            "roots": {
                "bookmark_bar": {
                    "type": "folder",
                    "children": [
                        {
                            "type": "url",
                            "name": "Example",
                            "url": "https://example.com"
                        }
                    ]
                }
            }
        }"#;

        let bookmarks: ChromeBookmarks = serde_json::from_str(json).unwrap();
        assert!(bookmarks.roots.is_some());

        let roots = bookmarks.roots.unwrap();
        assert!(roots.bookmark_bar.is_some());

        let bar = roots.bookmark_bar.unwrap();
        assert!(bar.children.is_some());
        assert_eq!(bar.children.unwrap().len(), 1);
    }

    #[test]
    fn test_process_folder() {
        let mut state = BookmarksState::new();

        let folder = ChromeBookmarkNode {
            name: Some("Test Folder".to_string()),
            url: None,
            node_type: Some("folder".to_string()),
            children: Some(vec![
                ChromeBookmarkNode {
                    name: Some("Test Bookmark".to_string()),
                    url: Some("https://test.com".to_string()),
                    node_type: Some("url".to_string()),
                    children: None,
                },
            ]),
        };

        state.process_chrome_folder(&folder);
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].name.as_str(), "Test Bookmark");
    }

    #[test]
    fn test_url_escaping() {
        let url = "https://example.com/path?query='test'";
        let command = format!("xdg-open '{}'", url.replace('\'', "'\\''"));
        assert!(command.contains("'\\''"));
    }
}
