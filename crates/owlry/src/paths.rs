//! Centralized path handling following XDG Base Directory Specification.
//!
//! XDG directories used:
//! - `$XDG_CONFIG_HOME/owlry/` - User configuration (config.toml, themes/, style.css)
//! - `$XDG_DATA_HOME/owlry/`   - User data (scripts/, frecency.json)
//! - `$XDG_CACHE_HOME/owlry/`  - Cache files (future use)
//!
//! See: https://specifications.freedesktop.org/basedir-spec/latest/

use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

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

/// Lua config file: `$XDG_CONFIG_HOME/owlry/owlry.lua`.
///
/// Takes precedence over `config.toml` when both exist (per
/// `docs/lua-api.md` §2 — Lua wins; TOML is ignored entirely in that case).
/// Named `owlry.lua` rather than `init.lua` for brand identity (D23).
pub fn lua_config_file() -> Option<PathBuf> {
    owlry_config_dir().map(|p| p.join("owlry.lua"))
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

/// The XDG-derived application directories only.
///
/// `$XDG_DATA_HOME/applications`, each `$XDG_DATA_DIRS` entry, and the two
/// spec defaults.
///
/// A subset of [`system_data_dirs`], excluding the Flatpak/Snap/Nix paths that
/// are appended unconditionally. Those are only real once their package manager
/// is installed, and their parents (`/var/lib/...`, `~/.nix-profile/share`,
/// `/run/current-system/sw/share`) are usually absent too — so they are watched
/// only when they already exist, never bootstrapped, which would mean watching
/// `/var/lib`, `$HOME` or `/run`.
pub fn xdg_application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add_dir = |path: PathBuf| {
        if seen.insert(path.clone()) {
            dirs.push(path);
        }
    };

    if let Some(data) = data_home() {
        add_dir(data.join("applications"));
    }
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in xdg_data_dirs.split(':') {
        if !dir.is_empty() {
            add_dir(PathBuf::from(dir).join("applications"));
        }
    }
    add_dir(PathBuf::from("/usr/share/applications"));
    add_dir(PathBuf::from("/usr/local/share/applications"));

    dirs
}

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

/// IPC socket path.
///
/// Resolution order:
/// 1. `$OWLRY_SOCKET` — full path override (useful for tests and side-by-side
///    daemon instances). The parent directory must be owned by the current
///    user and private, or a sticky directory such as `/tmp`.
/// 2. `$XDG_RUNTIME_DIR/owlry/owlry.sock` (the normal case). The owlry
///    subdirectory is created with mode 0700.
///
/// There is no `/tmp` fallback. Without `$XDG_RUNTIME_DIR` the caller gets a
/// diagnostic instead of a socket in a shared directory.
pub fn socket_path() -> io::Result<PathBuf> {
    if let Some(custom) = std::env::var_os("OWLRY_SOCKET") {
        let path = PathBuf::from(custom);
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "OWLRY_SOCKET must be an absolute path; got '{}'",
                    path.display()
                ),
            ));
        }
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("OWLRY_SOCKET '{}' has no parent directory", path.display()),
            )
        })?;
        validate_socket_parent(parent, false)?;
        return Ok(path);
    }

    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "XDG_RUNTIME_DIR is not set; set it or provide an absolute OWLRY_SOCKET path",
        )
    })?;
    let runtime_dir = PathBuf::from(runtime_dir);
    if !runtime_dir.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "XDG_RUNTIME_DIR must be an absolute path; got '{}'",
                runtime_dir.display()
            ),
        ));
    }

    let owlry_dir = runtime_dir.join(APP_NAME);
    validate_socket_parent(&owlry_dir, true)?;
    Ok(owlry_dir.join("owlry.sock"))
}

/// Validate (and create, when missing) the directory that will hold the
/// socket. The directory must be owned by the current user. A directory that
/// is writable by other users is only accepted when it carries the sticky
/// bit; that keeps `/tmp` usable for explicit `OWLRY_SOCKET` paths without
/// allowing another user to replace our socket there.
pub fn validate_socket_parent(parent: &Path, tighten: bool) -> io::Result<()> {
    if !parent.exists() {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(parent)?;
    }

    let metadata = std::fs::symlink_metadata(parent)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("socket parent '{}' is not a directory", parent.display()),
        ));
    }

    let ours = unsafe { libc::geteuid() };
    let mut mode = metadata.permissions().mode();
    if tighten && mode & 0o077 != 0 {
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        mode = 0o700;
    }
    let sticky = mode & 0o1000 != 0;

    // A root-owned sticky directory such as /tmp is acceptable for an
    // explicit OWLRY_SOCKET path. Its owner (root) is the only other user who
    // could unlink our socket, and the peer-uid check covers the pre-bind
    // case. An ordinary user's sticky directory is not accepted: that user
    // could remove or squat on our socket.
    if metadata.uid() != ours && !(!tighten && sticky && metadata.uid() == 0) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "socket parent '{}' is owned by uid {}, not current uid {ours}",
                parent.display(),
                metadata.uid()
            ),
        ));
    }

    let writable_by_others = mode & 0o022 != 0;
    if writable_by_others && !sticky {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "socket parent '{}' is writable by group or others (mode {:o})",
                parent.display(),
                mode & 0o7777
            ),
        ));
    }

    Ok(())
}

// =============================================================================
// Helper functions
// =============================================================================

/// Ensure parent directory of a file exists
pub fn ensure_parent_dir(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Identity of the socket inode we created.
///
/// Cleanup compares the device and inode before unlinking. This is a
/// best-effort guard, not an atomic ownership proof: the path can be replaced
/// between the check and the unlink, and a new file can reuse the same inode
/// number. The per-socket lock and the private directory are the primary
/// controls against replacement; same-UID processes are outside the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketIdentity {
    dev: u64,
    ino: u64,
}

impl SocketIdentity {
    pub fn from_path(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        Ok(Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }
}

/// Remove `path` only when it still refers to the inode described by
/// `identity`. Returns whether a file was removed.
///
/// This is not atomic. Callers rely on the per-socket lock and directory
/// permissions; the check cannot distinguish a replacement that reused the
/// same inode number.
pub fn remove_socket_if_owned(path: &Path, identity: SocketIdentity) -> io::Result<bool> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if metadata.dev() != identity.dev || metadata.ino() != identity.ino {
        return Ok(false);
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
    fn owlry_socket_env_overrides_xdg_runtime() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Save / restore so other tests in this module aren't affected.
        let prev = std::env::var_os("OWLRY_SOCKET");
        let dir = std::env::temp_dir().join(format!("owlry-path-test-{}", std::process::id()));
        let path = dir.join("foo.sock");
        unsafe { std::env::set_var("OWLRY_SOCKET", &path) };
        assert_eq!(socket_path().unwrap(), path);
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = std::fs::remove_dir_all(&dir);
        match prev {
            Some(v) => unsafe { std::env::set_var("OWLRY_SOCKET", v) },
            None => unsafe { std::env::remove_var("OWLRY_SOCKET") },
        }
    }

    #[test]
    fn relative_socket_override_is_rejected() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("OWLRY_SOCKET");
        unsafe { std::env::set_var("OWLRY_SOCKET", "relative.sock") };
        let err = socket_path().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        match prev {
            Some(v) => unsafe { std::env::set_var("OWLRY_SOCKET", v) },
            None => unsafe { std::env::remove_var("OWLRY_SOCKET") },
        }
    }

    #[test]
    fn missing_runtime_dir_is_an_error_not_a_tmp_fallback() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev_socket = std::env::var_os("OWLRY_SOCKET");
        let prev_runtime = std::env::var_os("XDG_RUNTIME_DIR");
        unsafe {
            std::env::remove_var("OWLRY_SOCKET");
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let err = socket_path().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        match prev_socket {
            Some(v) => unsafe { std::env::set_var("OWLRY_SOCKET", v) },
            None => unsafe { std::env::remove_var("OWLRY_SOCKET") },
        }
        match prev_runtime {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }

    #[test]
    fn default_runtime_dir_is_tightened_to_0700() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = ENV_LOCK.lock().unwrap();
        let runtime = std::env::temp_dir().join(format!(
            "owlry-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let owlry_dir = runtime.join("owlry");
        std::fs::create_dir_all(&owlry_dir).unwrap();
        std::fs::set_permissions(&owlry_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        let prev_socket = std::env::var_os("OWLRY_SOCKET");
        let prev_runtime = std::env::var_os("XDG_RUNTIME_DIR");
        unsafe {
            std::env::remove_var("OWLRY_SOCKET");
            std::env::set_var("XDG_RUNTIME_DIR", &runtime);
        }
        let path = socket_path().unwrap();
        assert_eq!(path, owlry_dir.join("owlry.sock"));
        let mode = std::fs::metadata(&owlry_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);

        match prev_socket {
            Some(v) => unsafe { std::env::set_var("OWLRY_SOCKET", v) },
            None => unsafe { std::env::remove_var("OWLRY_SOCKET") },
        }
        match prev_runtime {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
        let _ = std::fs::remove_dir_all(&runtime);
    }

    #[test]
    fn socket_parent_symlink_is_rejected() {
        let _guard = ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "owlry-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let prev = std::env::var_os("OWLRY_SOCKET");
        unsafe { std::env::set_var("OWLRY_SOCKET", link.join("foo.sock")) };
        let err = socket_path().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        match prev {
            Some(v) => unsafe { std::env::set_var("OWLRY_SOCKET", v) },
            None => unsafe { std::env::remove_var("OWLRY_SOCKET") },
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn socket_parent_that_is_writable_without_sticky_is_rejected() {
        let _guard = ENV_LOCK.lock().unwrap();
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("owlry-unsafe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();

        let prev = std::env::var_os("OWLRY_SOCKET");
        let path = dir.join("foo.sock");
        unsafe { std::env::set_var("OWLRY_SOCKET", &path) };
        let err = socket_path().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        match prev {
            Some(v) => unsafe { std::env::set_var("OWLRY_SOCKET", v) },
            None => unsafe { std::env::remove_var("OWLRY_SOCKET") },
        }
        let _ = std::fs::remove_dir_all(&dir);
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
