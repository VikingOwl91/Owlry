//! Scripts Plugin for Owlry
//!
//! A static provider that scans `~/.local/share/owlry/scripts/` for executable
//! scripts and provides them as launch items.
//!
//! Scripts can include a description by adding a comment after the shebang:
//! ```bash
//! #!/bin/bash
//! # This is my script description
//! echo "Hello"
//! ```

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

// Plugin metadata
const PLUGIN_ID: &str = "scripts";
const PLUGIN_NAME: &str = "Scripts";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Run user scripts from ~/.local/share/owlry/scripts/";

// Provider metadata
const PROVIDER_ID: &str = "scripts";
const PROVIDER_NAME: &str = "Scripts";
const PROVIDER_PREFIX: &str = ":script";
const PROVIDER_ICON: &str = "utilities-terminal";
const PROVIDER_TYPE_ID: &str = "scripts";

/// Scripts provider state - holds cached items
struct ScriptsState {
    items: Vec<PluginItem>,
}

impl ScriptsState {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn scripts_dir() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("owlry").join("scripts"))
    }

    fn load_scripts(&mut self) {
        self.items.clear();

        let scripts_dir = match Self::scripts_dir() {
            Some(p) => p,
            None => return,
        };

        if !scripts_dir.exists() {
            // Create the directory for the user
            let _ = fs::create_dir_all(&scripts_dir);
            return;
        }

        let entries = match fs::read_dir(&scripts_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check if executable
            let metadata = match path.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_executable = metadata.permissions().mode() & 0o111 != 0;
            if !is_executable {
                continue;
            }

            // Get script name without extension
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(filename.clone());

            // Try to read description from first line comment
            let description = Self::read_script_description(&path);

            // Determine icon based on extension or shebang
            let icon = Self::determine_icon(&path);

            let mut item = PluginItem::new(
                format!("script:{}", filename),
                format!("Script: {}", name),
                path.to_string_lossy().to_string(),
            )
            .with_icon(icon)
            .with_keywords(vec!["script".to_string()]);

            if let Some(desc) = description {
                item = item.with_description(desc);
            }

            self.items.push(item);
        }
    }

    fn read_script_description(path: &PathBuf) -> Option<String> {
        let content = fs::read_to_string(path).ok()?;
        let mut lines = content.lines();

        // Skip shebang if present
        let first_line = lines.next()?;
        let check_line = if first_line.starts_with("#!") {
            lines.next()?
        } else {
            first_line
        };

        // Look for a comment description
        if let Some(desc) = check_line.strip_prefix("# ") {
            Some(desc.trim().to_string())
        } else { check_line.strip_prefix("// ").map(|desc| desc.trim().to_string()) }
    }

    fn determine_icon(path: &PathBuf) -> String {
        // Check extension first
        if let Some(ext) = path.extension() {
            match ext.to_string_lossy().as_ref() {
                "sh" | "bash" | "zsh" => return "utilities-terminal".to_string(),
                "py" | "python" => return "text-x-python".to_string(),
                "js" | "ts" => return "text-x-javascript".to_string(),
                "rb" => return "text-x-ruby".to_string(),
                "pl" => return "text-x-perl".to_string(),
                _ => {}
            }
        }

        // Check shebang
        if let Ok(content) = fs::read_to_string(path)
            && let Some(first_line) = content.lines().next() {
                if first_line.contains("bash") || first_line.contains("sh") {
                    return "utilities-terminal".to_string();
                } else if first_line.contains("python") {
                    return "text-x-python".to_string();
                } else if first_line.contains("node") {
                    return "text-x-javascript".to_string();
                } else if first_line.contains("ruby") {
                    return "text-x-ruby".to_string();
                }
            }

        "application-x-executable".to_string()
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
        provider_type: ProviderKind::Static,
        type_id: RString::from(PROVIDER_TYPE_ID),
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(ScriptsState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<ScriptsState>
    let state = unsafe { &mut *(handle.ptr as *mut ScriptsState) };

    // Load scripts
    state.load_scripts();

    // Return items
    state.items.to_vec().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query is handled by the core using cached items
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<ScriptsState>
        unsafe {
            handle.drop_as::<ScriptsState>();
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
    fn test_scripts_state_new() {
        let state = ScriptsState::new();
        assert!(state.items.is_empty());
    }

    #[test]
    fn test_determine_icon_sh() {
        let path = PathBuf::from("/test/script.sh");
        let icon = ScriptsState::determine_icon(&path);
        assert_eq!(icon, "utilities-terminal");
    }

    #[test]
    fn test_determine_icon_python() {
        let path = PathBuf::from("/test/script.py");
        let icon = ScriptsState::determine_icon(&path);
        assert_eq!(icon, "text-x-python");
    }

    #[test]
    fn test_determine_icon_js() {
        let path = PathBuf::from("/test/script.js");
        let icon = ScriptsState::determine_icon(&path);
        assert_eq!(icon, "text-x-javascript");
    }

    #[test]
    fn test_determine_icon_unknown() {
        let path = PathBuf::from("/test/script.xyz");
        let icon = ScriptsState::determine_icon(&path);
        assert_eq!(icon, "application-x-executable");
    }

    #[test]
    fn test_scripts_dir() {
        // Should return Some path
        let dir = ScriptsState::scripts_dir();
        assert!(dir.is_some());
        assert!(dir.unwrap().ends_with("owlry/scripts"));
    }
}
