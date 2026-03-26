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

/// User plugins directory: `$XDG_CONFIG_HOME/owlry/plugins/`
///
/// Plugins are stored in config because they contain user-installed code
/// that the user explicitly chose to add (similar to themes).
pub fn plugins_dir() -> Option<PathBuf> {
    owlry_config_dir().map(|p| p.join("plugins"))
}

/// Frecency data file: `$XDG_DATA_HOME/owlry/frecency.json`
pub fn frecency_file() -> Option<PathBuf> {
    owlry_data_dir().map(|p| p.join("frecency.json"))
}

// =============================================================================
// System directories
// =============================================================================

/// System data directories for applications (XDG_DATA_DIRS)
///
/// Follows the XDG Base Directory Specification:
/// - $XDG_DATA_HOME/applications (defaults to ~/.local/share/applications)
/// - $XDG_DATA_DIRS/*/applications (defaults to /usr/local/share:/usr/share)
/// - Additional Flatpak and Snap directories
pub fn system_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Helper to add unique directories
    let mut add_dir = |path: PathBuf| {
        if seen.insert(path.clone()) {
            dirs.push(path);
        }
    };

    // 1. User data directory first (highest priority)
    if let Some(data) = data_home() {
        add_dir(data.join("applications"));
    }

    // 2. XDG_DATA_DIRS - parse the environment variable
    // Default per spec: /usr/local/share:/usr/share
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());

    for dir in xdg_data_dirs.split(':') {
        if !dir.is_empty() {
            add_dir(PathBuf::from(dir).join("applications"));
        }
    }

    // 3. Always include standard system directories as fallback
    // Some environments set XDG_DATA_DIRS without including these
    add_dir(PathBuf::from("/usr/share/applications"));
    add_dir(PathBuf::from("/usr/local/share/applications"));

    // 4. Flatpak directories (user and system)
    if let Some(data) = data_home() {
        add_dir(data.join("flatpak/exports/share/applications"));
    }
    add_dir(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    // 5. Snap directories
    add_dir(PathBuf::from("/var/lib/snapd/desktop/applications"));

    // 6. Nix directories (common on NixOS)
    if let Some(home) = dirs::home_dir() {
        add_dir(home.join(".nix-profile/share/applications"));
    }
    add_dir(PathBuf::from("/run/current-system/sw/share/applications"));

    dirs
}

// =============================================================================
// Runtime files
// =============================================================================

/// IPC socket path: `$XDG_RUNTIME_DIR/owlry/owlry.sock`
///
/// Falls back to `/tmp` if `$XDG_RUNTIME_DIR` is not set.
pub fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    runtime_dir.join(APP_NAME).join("owlry.sock")
}

// =============================================================================
// Helper functions
// =============================================================================

/// Ensure parent directory of a file exists
pub fn ensure_parent_dir(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists() {
            std::fs::create_dir_all(parent)?;
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
