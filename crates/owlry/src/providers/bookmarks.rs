//! Browser bookmarks provider.
//!
//! Reads bookmarks from Firefox (`places.sqlite` via `rusqlite`) and
//! Chromium-family browsers (Chrome, Chromium, Brave, Edge — JSON
//! `Bookmarks` files). Items launch via `xdg-open <url>`.
//!
//! This is a static provider: `refresh()` populates `self.items`, and the
//! core fuzzy-matches against the cached list.

use super::{ItemSource, LaunchItem, Provider, ProviderType};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const TYPE_ID: &str = "bookmarks";
const ICON: &str = "user-bookmarks-symbolic";

pub struct BookmarksProvider {
    items: Vec<LaunchItem>,
}

impl Default for BookmarksProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl BookmarksProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Cache directory for downloaded Firefox favicons.
    fn favicon_cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("owlry/favicons"))
    }

    fn ensure_favicon_cache_dir() -> Option<PathBuf> {
        Self::favicon_cache_dir().and_then(|dir| {
            fs::create_dir_all(&dir).ok()?;
            Some(dir)
        })
    }

    /// Hash a URL into a stable cache filename. Collisions are acceptable —
    /// the worst case is a favicon shown for the wrong bookmark.
    fn url_to_cache_filename(url: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut hasher);
        format!("{:016x}.png", hasher.finish())
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
            if firefox_dir.exists()
                && let Ok(entries) = fs::read_dir(&firefox_dir)
            {
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
        paths
    }

    /// Sibling `favicons.sqlite` next to a given `places.sqlite`, if present.
    fn firefox_favicons_path(places_path: &Path) -> Option<PathBuf> {
        let favicons = places_path.parent()?.join("favicons.sqlite");
        if favicons.exists() {
            Some(favicons)
        } else {
            None
        }
    }

    fn read_chrome_bookmarks(path: &Path, items: &mut Vec<LaunchItem>) {
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
                Self::process_chrome_folder(&bar, items);
            }
            if let Some(other) = roots.other {
                Self::process_chrome_folder(&other, items);
            }
            if let Some(synced) = roots.synced {
                Self::process_chrome_folder(&synced, items);
            }
        }
    }

    fn process_chrome_folder(folder: &ChromeBookmarkNode, items: &mut Vec<LaunchItem>) {
        if let Some(ref children) = folder.children {
            for child in children {
                match child.node_type.as_deref() {
                    Some("url") => {
                        if let Some(ref url) = child.url {
                            let name = child.name.clone().unwrap_or_else(|| url.clone());
                            items.push(LaunchItem {
                                id: format!("bookmark:{}", url),
                                name,
                                description: Some(url.clone()),
                                icon: Some(ICON.to_string()),
                                provider: ProviderType::Plugin(TYPE_ID.into()),
                                command: format!("xdg-open '{}'", url.replace('\'', "'\\''")),
                                terminal: false,
                                tags: vec!["bookmark".to_string(), "chrome".to_string()],
                                source: ItemSource::Core,
                            });
                        }
                    }
                    Some("folder") => {
                        Self::process_chrome_folder(child, items);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Read Firefox bookmarks via `rusqlite`. Copies the DB (+ WAL) into a
    /// temp file first so we can open in read-only mode without contesting
    /// the lock Firefox holds while running.
    fn read_firefox_bookmarks(places_path: &Path, items: &mut Vec<LaunchItem>) {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let temp_db = temp_dir.join(format!("owlry_places_{}.sqlite", pid));

        if fs::copy(places_path, &temp_db).is_err() {
            return;
        }

        let wal_path = places_path.with_extension("sqlite-wal");
        if wal_path.exists() {
            let temp_wal = temp_db.with_extension("sqlite-wal");
            let _ = fs::copy(&wal_path, &temp_wal);
        }

        let favicons_path = Self::firefox_favicons_path(places_path);
        let temp_favicons = temp_dir.join(format!("owlry_favicons_{}.sqlite", pid));
        if let Some(ref fp) = favicons_path {
            let _ = fs::copy(fp, &temp_favicons);
            let fav_wal = fp.with_extension("sqlite-wal");
            if fav_wal.exists() {
                let _ = fs::copy(&fav_wal, temp_favicons.with_extension("sqlite-wal"));
            }
        }

        let cache_dir = Self::ensure_favicon_cache_dir();
        let bookmarks = Self::fetch_firefox_bookmarks(&temp_db, &temp_favicons, cache_dir.as_ref());

        let _ = fs::remove_file(&temp_db);
        let _ = fs::remove_file(temp_db.with_extension("sqlite-wal"));
        let _ = fs::remove_file(&temp_favicons);
        let _ = fs::remove_file(temp_favicons.with_extension("sqlite-wal"));

        for (title, url, favicon_path) in bookmarks {
            let icon = favicon_path.unwrap_or_else(|| ICON.to_string());
            items.push(LaunchItem {
                id: format!("bookmark:firefox:{}", url),
                name: title,
                description: Some(url.clone()),
                icon: Some(icon),
                provider: ProviderType::Plugin(TYPE_ID.into()),
                command: format!("xdg-open '{}'", url.replace('\'', "'\\''")),
                terminal: false,
                tags: vec!["bookmark".to_string(), "firefox".to_string()],
                source: ItemSource::Core,
            });
        }
    }

    /// Run the bookmark SELECT against `places.sqlite` and, if available,
    /// fetch + cache favicons from `favicons.sqlite`.
    fn fetch_firefox_bookmarks(
        places_path: &Path,
        favicons_path: &Path,
        cache_dir: Option<&PathBuf>,
    ) -> Vec<(String, String, Option<String>)> {
        let conn = match Connection::open_with_flags(
            places_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // type=1 means URL bookmarks (not folders, separators, etc.). We
        // skip Firefox-internal `place:` and `about:` URLs.
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

        let mut stmt = match conn.prepare(query) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let bookmarks: Vec<(String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        let cache_dir = match cache_dir {
            Some(c) => c,
            None => return bookmarks.into_iter().map(|(t, u)| (t, u, None)).collect(),
        };

        let fav_conn = match Connection::open_with_flags(
            favicons_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => return bookmarks.into_iter().map(|(t, u)| (t, u, None)).collect(),
        };

        let mut results = Vec::new();
        for (title, url) in bookmarks {
            let favicon_path = Self::get_favicon_for_url(&fav_conn, &url, cache_dir);
            results.push((title, url, favicon_path));
        }

        results
    }

    /// Look up a favicon for `page_url` in Firefox's `favicons.sqlite`, writing
    /// the blob to the cache directory on first hit. Returns the cached path
    /// (or None if no favicon / write failed).
    fn get_favicon_for_url(conn: &Connection, page_url: &str, cache_dir: &Path) -> Option<String> {
        let cache_filename = Self::url_to_cache_filename(page_url);
        let cache_path = cache_dir.join(&cache_filename);
        if cache_path.exists() {
            return Some(cache_path.to_string_lossy().to_string());
        }

        // Prefer the 32px-ish icon if multiple sizes exist.
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

        let data: Option<Vec<u8>> = conn.query_row(query, [page_url], |row| row.get(0)).ok();

        let data = data?;
        if data.is_empty() {
            return None;
        }

        let mut file = fs::File::create(&cache_path).ok()?;
        file.write_all(&data).ok()?;

        Some(cache_path.to_string_lossy().to_string())
    }
}

impl Provider for BookmarksProvider {
    fn name(&self) -> &str {
        "Bookmarks"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        self.items.clear();

        for path in Self::chromium_bookmark_paths() {
            if path.exists() {
                Self::read_chrome_bookmarks(&path, &mut self.items);
            }
        }

        for path in Self::firefox_places_paths() {
            Self::read_firefox_bookmarks(&path, &mut self.items);
        }
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn prefix(&self) -> Option<&str> {
        Some(":bm")
    }

    fn icon(&self) -> &str {
        ICON
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Bookmarks")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("bookmarks")
    }
}

// Chrome's `Bookmarks` JSON layout.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmarks_state_new() {
        let p = BookmarksProvider::new();
        assert!(p.items().is_empty());
    }

    #[test]
    fn test_chromium_paths() {
        let paths = BookmarksProvider::chromium_bookmark_paths();
        // Should have at least some paths configured (5 known browsers).
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_firefox_paths() {
        // Path detection should not panic, even if Firefox isn't installed.
        let paths = BookmarksProvider::firefox_places_paths();
        let _ = paths.len();
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
        let mut items = Vec::new();

        let folder = ChromeBookmarkNode {
            name: Some("Test Folder".to_string()),
            url: None,
            node_type: Some("folder".to_string()),
            children: Some(vec![ChromeBookmarkNode {
                name: Some("Test Bookmark".to_string()),
                url: Some("https://test.com".to_string()),
                node_type: Some("url".to_string()),
                children: None,
            }]),
        };

        BookmarksProvider::process_chrome_folder(&folder, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Test Bookmark");
        assert_eq!(items[0].provider, ProviderType::Plugin("bookmarks".into()));
        assert_eq!(items[0].source, ItemSource::Core);
        assert!(items[0].command.starts_with("xdg-open '"));
    }

    #[test]
    fn test_url_escaping() {
        let url = "https://example.com/path?query='test'";
        let command = format!("xdg-open '{}'", url.replace('\'', "'\\''"));
        assert!(command.contains("'\\''"));
    }

    #[test]
    fn provider_type_is_bookmarks_plugin() {
        let p = BookmarksProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("bookmarks".into()));
    }

    #[test]
    fn provider_prefix_is_bm() {
        let p = BookmarksProvider::new();
        assert_eq!(p.prefix(), Some(":bm"));
    }
}
