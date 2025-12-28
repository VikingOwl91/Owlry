use crate::providers::{LaunchItem, Provider, ProviderType};
use log::{debug, warn};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// Custom scripts provider - runs user scripts from ~/.config/owlry/scripts/
pub struct ScriptsProvider {
    items: Vec<LaunchItem>,
}

impl ScriptsProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn scripts_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("owlry").join("scripts"))
    }

    fn load_scripts(&mut self) {
        self.items.clear();

        let scripts_dir = match Self::scripts_dir() {
            Some(p) => p,
            None => {
                debug!("Could not determine scripts directory");
                return;
            }
        };

        if !scripts_dir.exists() {
            debug!("Scripts directory not found at {:?}", scripts_dir);
            // Create the directory for the user
            if let Err(e) = fs::create_dir_all(&scripts_dir) {
                warn!("Failed to create scripts directory: {}", e);
            }
            return;
        }

        let entries = match fs::read_dir(&scripts_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read scripts directory: {}", e);
                return;
            }
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

            self.items.push(LaunchItem {
                id: format!("script:{}", filename),
                name: format!("Script: {}", name),
                description,
                icon: Some(icon),
                provider: ProviderType::Scripts,
                command: path.to_string_lossy().to_string(),
                terminal: false,
            });
        }

        debug!("Loaded {} scripts from {:?}", self.items.len(), scripts_dir);
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
        if check_line.starts_with("# ") {
            Some(check_line[2..].trim().to_string())
        } else if check_line.starts_with("// ") {
            Some(check_line[3..].trim().to_string())
        } else {
            None
        }
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
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(first_line) = content.lines().next() {
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
        }

        "application-x-executable".to_string()
    }
}

impl Provider for ScriptsProvider {
    fn name(&self) -> &str {
        "Scripts"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Scripts
    }

    fn refresh(&mut self) {
        self.load_scripts();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scripts_provider() {
        let mut provider = ScriptsProvider::new();
        provider.refresh();
        // Just ensure it doesn't panic
    }
}
