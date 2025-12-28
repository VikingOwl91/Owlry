use crate::providers::{LaunchItem, Provider, ProviderType};
use log::{debug, warn};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Browser bookmarks provider - reads Firefox and Chrome bookmarks
pub struct BookmarksProvider {
    items: Vec<LaunchItem>,
}

impl BookmarksProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn load_bookmarks(&mut self) {
        self.items.clear();

        // Try Firefox first, then Chrome/Chromium
        self.load_firefox_bookmarks();
        self.load_chrome_bookmarks();

        debug!("Loaded {} bookmarks total", self.items.len());
    }

    fn load_firefox_bookmarks(&mut self) {
        // Firefox stores bookmarks in places.sqlite
        // The file is locked when Firefox is running, so we read from backup
        let firefox_dir = match dirs::home_dir() {
            Some(h) => h.join(".mozilla").join("firefox"),
            None => return,
        };

        if !firefox_dir.exists() {
            debug!("Firefox directory not found");
            return;
        }

        // Find default profile (ends with .default-release or .default)
        let profile_dir = match Self::find_firefox_profile(&firefox_dir) {
            Some(p) => p,
            None => {
                debug!("No Firefox profile found");
                return;
            }
        };

        // Try to read bookmarkbackups (JSON format, not locked)
        let backup_dir = profile_dir.join("bookmarkbackups");
        if backup_dir.exists() {
            if let Some(latest_backup) = Self::find_latest_file(&backup_dir, "jsonlz4") {
                // jsonlz4 files need decompression - skip for now, try places.sqlite
                debug!("Found Firefox backup at {:?}, but jsonlz4 not supported", latest_backup);
            }
        }

        // Try places.sqlite directly (may fail if Firefox is running)
        let places_db = profile_dir.join("places.sqlite");
        if places_db.exists() {
            self.read_firefox_places(&places_db);
        }
    }

    fn find_firefox_profile(firefox_dir: &PathBuf) -> Option<PathBuf> {
        let entries = fs::read_dir(firefox_dir).ok()?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".default-release") || name.ends_with(".default") {
                return Some(entry.path());
            }
        }
        None
    }

    fn find_latest_file(dir: &PathBuf, extension: &str) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;

        entries
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == extension)
                    .unwrap_or(false)
            })
            .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
            .map(|e| e.path())
    }

    fn read_firefox_places(&mut self, db_path: &PathBuf) {
        // Note: This requires the rusqlite crate which we don't have
        // For now, skip Firefox SQLite reading
        debug!(
            "Firefox places.sqlite found at {:?}, but SQLite reading not implemented",
            db_path
        );
    }

    fn load_chrome_bookmarks(&mut self) {
        // Chrome/Chromium bookmarks are in JSON format
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };

        // Try multiple browser paths
        let bookmark_paths = [
            // Chrome
            home.join(".config/google-chrome/Default/Bookmarks"),
            // Chromium
            home.join(".config/chromium/Default/Bookmarks"),
            // Brave
            home.join(".config/BraveSoftware/Brave-Browser/Default/Bookmarks"),
            // Edge
            home.join(".config/microsoft-edge/Default/Bookmarks"),
            // Vivaldi
            home.join(".config/vivaldi/Default/Bookmarks"),
        ];

        for path in &bookmark_paths {
            if path.exists() {
                self.read_chrome_bookmarks(path);
            }
        }
    }

    fn read_chrome_bookmarks(&mut self, path: &PathBuf) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read Chrome bookmarks from {:?}: {}", path, e);
                return;
            }
        };

        let bookmarks: ChromeBookmarks = match serde_json::from_str(&content) {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to parse Chrome bookmarks: {}", e);
                return;
            }
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

        debug!("Loaded Chrome bookmarks from {:?}", path);
    }

    fn process_chrome_folder(&mut self, folder: &ChromeBookmarkNode) {
        if let Some(ref children) = folder.children {
            for child in children {
                match child.node_type.as_deref() {
                    Some("url") => {
                        if let Some(ref url) = child.url {
                            let name = child.name.clone().unwrap_or_else(|| url.clone());

                            self.items.push(LaunchItem {
                                id: format!("bookmark:{}", url),
                                name,
                                description: Some(url.clone()),
                                icon: Some("web-browser".to_string()),
                                provider: ProviderType::Bookmarks,
                                command: format!("xdg-open '{}'", url.replace('\'', "'\\''")),
                                terminal: false,
                            });
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

impl Provider for BookmarksProvider {
    fn name(&self) -> &str {
        "Bookmarks"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Bookmarks
    }

    fn refresh(&mut self) {
        self.load_bookmarks();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmarks_provider() {
        let mut provider = BookmarksProvider::new();
        provider.refresh();
        // Just ensure it doesn't panic
    }
}
