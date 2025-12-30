//! Bookmarks Plugin for Owlry
//!
//! A static provider that reads browser bookmarks from various browsers.
//!
//! Supported browsers:
//! - Firefox (via places.sqlite using SQLx)
//! - Chrome
//! - Chromium
//! - Brave
//! - Edge

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};
use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

// Plugin metadata
const PLUGIN_ID: &str = "bookmarks";
const PLUGIN_NAME: &str = "Bookmarks";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Browser bookmark search";

// Provider metadata
const PROVIDER_ID: &str = "bookmarks";
const PROVIDER_NAME: &str = "Bookmarks";
const PROVIDER_PREFIX: &str = ":bm";
const PROVIDER_ICON: &str = "user-bookmarks-symbolic";
const PROVIDER_TYPE_ID: &str = "bookmarks";

/// Bookmarks provider state - holds cached items
struct BookmarksState {
    /// Cached bookmark items (returned immediately on refresh)
    items: Vec<PluginItem>,
    /// Flag to prevent concurrent background loads
    loading: Arc<AtomicBool>,
}

impl BookmarksState {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            loading: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get or create the favicon cache directory
    fn favicon_cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("owlry/favicons"))
    }

    /// Ensure the favicon cache directory exists
    fn ensure_favicon_cache_dir() -> Option<PathBuf> {
        Self::favicon_cache_dir().and_then(|dir| {
            fs::create_dir_all(&dir).ok()?;
            Some(dir)
        })
    }

    /// Hash a URL to create a cache filename
    fn url_to_cache_filename(url: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut hasher);
        format!("{:016x}.png", hasher.finish())
    }

    /// Get the bookmark cache file path
    fn bookmark_cache_file() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("owlry/bookmarks.json"))
    }

    /// Load cached bookmarks from disk (fast)
    fn load_cached_bookmarks() -> Vec<PluginItem> {
        let cache_file = match Self::bookmark_cache_file() {
            Some(f) => f,
            None => return Vec::new(),
        };

        if !cache_file.exists() {
            return Vec::new();
        }

        let content = match fs::read_to_string(&cache_file) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Parse cached bookmarks (simple JSON format)
        #[derive(serde::Deserialize)]
        struct CachedBookmark {
            id: String,
            name: String,
            command: String,
            description: Option<String>,
            icon: String,
        }

        let cached: Vec<CachedBookmark> = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        cached
            .into_iter()
            .map(|b| {
                let mut item = PluginItem::new(b.id, b.name, b.command)
                    .with_icon(&b.icon)
                    .with_keywords(vec!["bookmark".to_string()]);
                if let Some(desc) = b.description {
                    item = item.with_description(desc);
                }
                item
            })
            .collect()
    }

    /// Save bookmarks to cache file
    fn save_cached_bookmarks(items: &[PluginItem]) {
        let cache_file = match Self::bookmark_cache_file() {
            Some(f) => f,
            None => return,
        };

        // Ensure cache directory exists
        if let Some(parent) = cache_file.parent() {
            let _ = fs::create_dir_all(parent);
        }

        #[derive(serde::Serialize)]
        struct CachedBookmark {
            id: String,
            name: String,
            command: String,
            description: Option<String>,
            icon: String,
        }

        let cached: Vec<CachedBookmark> = items
            .iter()
            .map(|item| {
                let desc: Option<String> = match &item.description {
                    abi_stable::std_types::ROption::RSome(s) => Some(s.to_string()),
                    abi_stable::std_types::ROption::RNone => None,
                };
                let icon: String = match &item.icon {
                    abi_stable::std_types::ROption::RSome(s) => s.to_string(),
                    abi_stable::std_types::ROption::RNone => PROVIDER_ICON.to_string(),
                };
                CachedBookmark {
                    id: item.id.to_string(),
                    name: item.name.to_string(),
                    command: item.command.to_string(),
                    description: desc,
                    icon,
                }
            })
            .collect();

        if let Ok(json) = serde_json::to_string(&cached) {
            let _ = fs::write(&cache_file, json);
        }
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

    fn firefox_places_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            let firefox_dir = home.join(".mozilla/firefox");
            if firefox_dir.exists() {
                // Find all profile directories
                if let Ok(entries) = fs::read_dir(&firefox_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let places = path.join("places.sqlite");
                            if places.exists() {
                                paths.push(places);
                            }
                        }
                    }
                }
            }
        }

        paths
    }

    /// Find Firefox favicons.sqlite paths (paired with places.sqlite)
    fn firefox_favicons_path(places_path: &Path) -> Option<PathBuf> {
        let favicons = places_path.parent()?.join("favicons.sqlite");
        if favicons.exists() {
            Some(favicons)
        } else {
            None
        }
    }

    fn load_bookmarks(&mut self) {
        // Fast path: load from cache immediately
        if self.items.is_empty() {
            self.items = Self::load_cached_bookmarks();
        }

        // Don't start another background load if one is already running
        if self.loading.swap(true, Ordering::SeqCst) {
            return;
        }

        // Spawn background thread to refresh bookmarks
        let loading = self.loading.clone();
        thread::spawn(move || {
            let mut items = Vec::new();

            // Load Chrome/Chromium bookmarks (fast - just JSON parsing)
            for path in Self::chromium_bookmark_paths() {
                if path.exists() {
                    Self::read_chrome_bookmarks_static(&path, &mut items);
                }
            }

            // Load Firefox bookmarks with favicons (async via tokio)
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => {
                    loading.store(false, Ordering::SeqCst);
                    return;
                }
            };

            for path in Self::firefox_places_paths() {
                rt.block_on(async {
                    Self::read_firefox_bookmarks_async(&path, &mut items).await;
                });
            }

            // Save to cache for next startup
            Self::save_cached_bookmarks(&items);

            loading.store(false, Ordering::SeqCst);
        });
    }

    /// Read Chrome bookmarks (static helper for background thread)
    fn read_chrome_bookmarks_static(path: &PathBuf, items: &mut Vec<PluginItem>) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let bookmarks: ChromeBookmarks = match serde_json::from_str(&content) {
            Ok(b) => b,
            Err(_) => return,
        };

        if let Some(roots) = bookmarks.roots {
            if let Some(bar) = roots.bookmark_bar {
                Self::process_chrome_folder_static(&bar, items);
            }
            if let Some(other) = roots.other {
                Self::process_chrome_folder_static(&other, items);
            }
            if let Some(synced) = roots.synced {
                Self::process_chrome_folder_static(&synced, items);
            }
        }
    }

    fn process_chrome_folder_static(folder: &ChromeBookmarkNode, items: &mut Vec<PluginItem>) {
        if let Some(ref children) = folder.children {
            for child in children {
                match child.node_type.as_deref() {
                    Some("url") => {
                        if let Some(ref url) = child.url {
                            let name = child.name.clone().unwrap_or_else(|| url.clone());
                            items.push(
                                PluginItem::new(
                                    format!("bookmark:{}", url),
                                    name,
                                    format!("xdg-open '{}'", url.replace('\'', "'\\''")),
                                )
                                .with_description(url.clone())
                                .with_icon(PROVIDER_ICON)
                                .with_keywords(vec!["bookmark".to_string(), "chrome".to_string()]),
                            );
                        }
                    }
                    Some("folder") => {
                        Self::process_chrome_folder_static(child, items);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Read Firefox bookmarks asynchronously
    async fn read_firefox_bookmarks_async(places_path: &PathBuf, items: &mut Vec<PluginItem>) {
        let temp_dir = std::env::temp_dir();
        let temp_db = temp_dir.join("owlry_places_temp.sqlite");

        if fs::copy(places_path, &temp_db).is_err() {
            return;
        }

        let wal_path = places_path.with_extension("sqlite-wal");
        if wal_path.exists() {
            let temp_wal = temp_db.with_extension("sqlite-wal");
            let _ = fs::copy(&wal_path, &temp_wal);
        }

        let favicons_path = Self::firefox_favicons_path(places_path);
        let temp_favicons = temp_dir.join("owlry_favicons_temp.sqlite");
        if let Some(ref fp) = favicons_path {
            let _ = fs::copy(fp, &temp_favicons);
            let fav_wal = fp.with_extension("sqlite-wal");
            if fav_wal.exists() {
                let _ = fs::copy(&fav_wal, temp_favicons.with_extension("sqlite-wal"));
            }
        }

        let db_url = format!("sqlite:{}?mode=ro", temp_db.display());
        let favicons_url = if favicons_path.is_some() {
            Some(format!("sqlite:{}?mode=ro", temp_favicons.display()))
        } else {
            None
        };

        let cache_dir = Self::ensure_favicon_cache_dir();

        let bookmarks = Self::fetch_firefox_bookmarks_with_favicons(
            &db_url,
            favicons_url.as_deref(),
            cache_dir.as_ref(),
        )
        .await;

        // Clean up temp files
        let _ = fs::remove_file(&temp_db);
        let _ = fs::remove_file(temp_db.with_extension("sqlite-wal"));
        let _ = fs::remove_file(&temp_favicons);
        let _ = fs::remove_file(temp_favicons.with_extension("sqlite-wal"));

        for (title, url, favicon_path) in bookmarks {
            let icon = favicon_path.unwrap_or_else(|| PROVIDER_ICON.to_string());
            items.push(
                PluginItem::new(
                    format!("bookmark:firefox:{}", url),
                    title,
                    format!("xdg-open '{}'", url.replace('\'', "'\\''")),
                )
                .with_description(url)
                .with_icon(&icon)
                .with_keywords(vec!["bookmark".to_string(), "firefox".to_string()]),
            );
        }
    }

    /// Fetch Firefox bookmarks with their favicons
    async fn fetch_firefox_bookmarks_with_favicons(
        places_url: &str,
        favicons_url: Option<&str>,
        cache_dir: Option<&PathBuf>,
    ) -> Vec<(String, String, Option<String>)> {
        // First, fetch bookmarks from places.sqlite
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect(places_url)
            .await
        {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        // Query bookmarks joining moz_bookmarks with moz_places
        // type=1 means URL bookmarks (not folders, separators, etc.)
        let query = r#"
            SELECT b.title, p.url
            FROM moz_bookmarks b
            JOIN moz_places p ON b.fk = p.id
            WHERE b.type = 1
              AND p.url NOT LIKE 'place:%'
              AND p.url NOT LIKE 'about:%'
              AND b.title IS NOT NULL
              AND b.title != ''
            ORDER BY b.dateAdded DESC
            LIMIT 500
        "#;

        let rows = match sqlx::query(query).fetch_all(&pool).await {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let bookmarks: Vec<(String, String)> = rows
            .into_iter()
            .filter_map(|row| {
                let title: Option<String> = row.get("title");
                let url: Option<String> = row.get("url");
                match (title, url) {
                    (Some(t), Some(u)) => Some((t, u)),
                    _ => None,
                }
            })
            .collect();

        // If no favicons database or cache dir, return without favicons
        let (favicons_url, cache_dir) = match (favicons_url, cache_dir) {
            (Some(f), Some(c)) => (f, c),
            _ => return bookmarks.into_iter().map(|(t, u)| (t, u, None)).collect(),
        };

        // Connect to favicons database
        let fav_pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect(favicons_url)
            .await
        {
            Ok(p) => p,
            Err(_) => return bookmarks.into_iter().map(|(t, u)| (t, u, None)).collect(),
        };

        // Fetch favicons for each URL
        let mut results = Vec::new();
        for (title, url) in bookmarks {
            let favicon_path = Self::get_favicon_for_url(&fav_pool, &url, cache_dir).await;
            results.push((title, url, favicon_path));
        }

        results
    }

    /// Get favicon for a URL, caching to file if needed
    async fn get_favicon_for_url(
        pool: &sqlx::SqlitePool,
        page_url: &str,
        cache_dir: &Path,
    ) -> Option<String> {
        // Check if already cached
        let cache_filename = Self::url_to_cache_filename(page_url);
        let cache_path = cache_dir.join(&cache_filename);
        if cache_path.exists() {
            return Some(cache_path.to_string_lossy().to_string());
        }

        // Query favicon data from database
        // Join moz_pages_w_icons -> moz_icons_to_pages -> moz_icons
        // Prefer smaller icons (32px) for efficiency
        let query = r#"
            SELECT i.data
            FROM moz_pages_w_icons p
            JOIN moz_icons_to_pages ip ON p.id = ip.page_id
            JOIN moz_icons i ON ip.icon_id = i.id
            WHERE p.page_url = ?
              AND i.data IS NOT NULL
            ORDER BY ABS(i.width - 32) ASC
            LIMIT 1
        "#;

        let row = sqlx::query(query)
            .bind(page_url)
            .fetch_optional(pool)
            .await
            .ok()??;

        let data: Vec<u8> = row.get("data");
        if data.is_empty() {
            return None;
        }

        // Write favicon data to cache file
        let mut file = fs::File::create(&cache_path).ok()?;
        file.write_all(&data).ok()?;

        Some(cache_path.to_string_lossy().to_string())
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
    fn test_firefox_paths() {
        // This will find paths if Firefox is installed
        let paths = BookmarksState::firefox_places_paths();
        // Path detection should work (may be empty if Firefox not installed)
        assert!(paths.len() >= 0);
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
