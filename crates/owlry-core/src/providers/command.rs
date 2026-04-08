use super::{ItemSource, LaunchItem, Provider, ProviderType};
use log::debug;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[derive(Default)]
pub struct CommandProvider {
    items: Vec<LaunchItem>,
}

impl CommandProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_path_dirs() -> Vec<PathBuf> {
        std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect()
    }

    fn is_executable(path: &std::path::Path) -> bool {
        if let Ok(metadata) = path.metadata() {
            let permissions = metadata.permissions();
            permissions.mode() & 0o111 != 0
        } else {
            false
        }
    }
}

impl Provider for CommandProvider {
    fn name(&self) -> &str {
        "Commands"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Command
    }

    fn refresh(&mut self) {
        self.items.clear();

        let dirs = Self::get_path_dirs();
        let mut seen_names: HashSet<String> = HashSet::new();

        debug!("Scanning PATH directories for commands");

        for dir in dirs {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();

                // Skip directories and non-executable files
                if path.is_dir() || !Self::is_executable(&path) {
                    continue;
                }

                let name = match path.file_name() {
                    Some(n) => n.to_string_lossy().to_string(),
                    None => continue,
                };

                // Skip duplicates (first one in PATH wins)
                if seen_names.contains(&name) {
                    continue;
                }
                seen_names.insert(name.clone());

                // Skip hidden files
                if name.starts_with('.') {
                    continue;
                }

                let item = LaunchItem {
                    id: path.to_string_lossy().to_string(),
                    name: name.clone(),
                    description: Some(format!("Run {}", path.display())),
                    icon: Some("utilities-terminal".to_string()),
                    provider: ProviderType::Command,
                    command: name,
                    terminal: false,
                    tags: Vec::new(),
                    source: ItemSource::Core,
                };

                self.items.push(item);
            }
        }

        debug!("Found {} commands in PATH", self.items.len());

        // Sort alphabetically
        self.items
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}
