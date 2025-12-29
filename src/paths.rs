//! Centralized path handling following XDG Base Directory Specification.
//!
//! XDG directories used:
//! - `$XDG_CONFIG_HOME/owlry/` - User configuration (config.toml, themes/, style.css)
//! - `$XDG_DATA_HOME/owlry/`   - User data (scripts/, frecency.json)
//! - `$XDG_CACHE_HOME/owlry/`  - Cache files (future use)
//!
//! See: https://specifications.freedesktop.org/basedir-spec/latest/

use std::path::PathBuf;

/// Application name used in XDG paths
const APP_NAME: &str = "owlry";

// =============================================================================
// XDG Base Directories
// =============================================================================

/// Get XDG config home: `$XDG_CONFIG_HOME` or `~/.config`
pub fn config_home() -> Option<PathBuf> {
    dirs::config_dir()
}

/// Get XDG data home: `$XDG_DATA_HOME` or `~/.local/share`
pub fn data_home() -> Option<PathBuf> {
    dirs::data_dir()
}

/// Get XDG cache home: `$XDG_CACHE_HOME` or `~/.cache`
#[allow(dead_code)]
pub fn cache_home() -> Option<PathBuf> {
    dirs::cache_dir()
}

/// Get user home directory
pub fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

// =============================================================================
// Owlry-specific directories
// =============================================================================

/// Owlry config directory: `$XDG_CONFIG_HOME/owlry/`
pub fn owlry_config_dir() -> Option<PathBuf> {
    config_home().map(|p| p.join(APP_NAME))
}

/// Owlry data directory: `$XDG_DATA_HOME/owlry/`
pub fn owlry_data_dir() -> Option<PathBuf> {
    data_home().map(|p| p.join(APP_NAME))
}

/// Owlry cache directory: `$XDG_CACHE_HOME/owlry/`
#[allow(dead_code)]
pub fn owlry_cache_dir() -> Option<PathBuf> {
    cache_home().map(|p| p.join(APP_NAME))
}

// =============================================================================
// Config files
// =============================================================================

/// Main config file: `$XDG_CONFIG_HOME/owlry/config.toml`
pub fn config_file() -> Option<PathBuf> {
    owlry_config_dir().map(|p| p.join("config.toml"))
}

/// Custom user stylesheet: `$XDG_CONFIG_HOME/owlry/style.css`
pub fn custom_style_file() -> Option<PathBuf> {
    owlry_config_dir().map(|p| p.join("style.css"))
}

/// User themes directory: `$XDG_CONFIG_HOME/owlry/themes/`
pub fn themes_dir() -> Option<PathBuf> {
    owlry_config_dir().map(|p| p.join("themes"))
}

/// Get path for a specific theme: `$XDG_CONFIG_HOME/owlry/themes/{name}.css`
pub fn theme_file(name: &str) -> Option<PathBuf> {
    themes_dir().map(|p| p.join(format!("{}.css", name)))
}

// =============================================================================
// Data files
// =============================================================================

/// User scripts directory: `$XDG_DATA_HOME/owlry/scripts/`
pub fn scripts_dir() -> Option<PathBuf> {
    owlry_data_dir().map(|p| p.join("scripts"))
}

/// Frecency data file: `$XDG_DATA_HOME/owlry/frecency.json`
pub fn frecency_file() -> Option<PathBuf> {
    owlry_data_dir().map(|p| p.join("frecency.json"))
}

// =============================================================================
// System directories
// =============================================================================

/// System data directories for applications (XDG_DATA_DIRS)
pub fn system_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // User data directory first
    if let Some(data) = data_home() {
        dirs.push(data.join("applications"));
    }

    // System directories
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/local/share/applications"));

    // Flatpak directories
    if let Some(data) = data_home() {
        dirs.push(data.join("flatpak/exports/share/applications"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    dirs
}

// =============================================================================
// External application paths
// =============================================================================

/// SSH config file: `~/.ssh/config`
pub fn ssh_config() -> Option<PathBuf> {
    home().map(|p| p.join(".ssh").join("config"))
}

/// Firefox profile directory: `~/.mozilla/firefox/`
pub fn firefox_dir() -> Option<PathBuf> {
    home().map(|p| p.join(".mozilla").join("firefox"))
}

/// Chromium-based browser bookmark paths (using XDG config where browsers support it)
pub fn chromium_bookmark_paths() -> Vec<PathBuf> {
    let config = match config_home() {
        Some(c) => c,
        None => return Vec::new(),
    };

    vec![
        // Google Chrome
        config.join("google-chrome/Default/Bookmarks"),
        // Chromium
        config.join("chromium/Default/Bookmarks"),
        // Brave
        config.join("BraveSoftware/Brave-Browser/Default/Bookmarks"),
        // Microsoft Edge
        config.join("microsoft-edge/Default/Bookmarks"),
        // Vivaldi
        config.join("vivaldi/Default/Bookmarks"),
    ]
}

// =============================================================================
// Helper functions
// =============================================================================

/// Ensure a directory exists, creating it if necessary
pub fn ensure_dir(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Ensure parent directory of a file exists
pub fn ensure_parent_dir(path: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_are_consistent() {
        // All owlry paths should be under XDG directories
        if let (Some(config), Some(data)) = (owlry_config_dir(), owlry_data_dir()) {
            assert!(config.ends_with("owlry"));
            assert!(data.ends_with("owlry"));
        }
    }

    #[test]
    fn test_config_file_path() {
        if let Some(path) = config_file() {
            assert!(path.ends_with("config.toml"));
            assert!(path.to_string_lossy().contains("owlry"));
        }
    }

    #[test]
    fn test_frecency_in_data_dir() {
        if let Some(path) = frecency_file() {
            assert!(path.ends_with("frecency.json"));
            // Should be in data dir, not config dir
            let path_str = path.to_string_lossy();
            assert!(
                path_str.contains(".local/share") || path_str.contains("XDG_DATA_HOME"),
                "frecency should be in data directory"
            );
        }
    }
}
