use super::{LaunchItem, Provider, ProviderType};
use freedesktop_desktop_entry::{DesktopEntry, Iter};
use log::{debug, warn};
use std::path::PathBuf;

pub struct ApplicationProvider {
    items: Vec<LaunchItem>,
}

impl ApplicationProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn get_application_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // User applications
        if let Some(data_home) = dirs::data_dir() {
            dirs.push(data_home.join("applications"));
        }

        // System applications
        dirs.push(PathBuf::from("/usr/share/applications"));
        dirs.push(PathBuf::from("/usr/local/share/applications"));

        // Flatpak applications
        if let Some(data_home) = dirs::data_dir() {
            dirs.push(data_home.join("flatpak/exports/share/applications"));
        }
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

        dirs
    }
}

impl Provider for ApplicationProvider {
    fn name(&self) -> &str {
        "Applications"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Application
    }

    fn refresh(&mut self) {
        self.items.clear();

        let dirs = Self::get_application_dirs();
        debug!("Scanning application directories: {:?}", dirs);

        // Empty locale list for default locale
        let locales: &[&str] = &[];

        for path in Iter::new(dirs.into_iter()) {
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read {:?}: {}", path, e);
                    continue;
                }
            };

            let desktop_entry = match DesktopEntry::from_str(&path, &content, Some(locales)) {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to parse {:?}: {}", path, e);
                    continue;
                }
            };

            // Skip entries marked as hidden or no-display
            if desktop_entry.no_display() || desktop_entry.hidden() {
                continue;
            }

            // Only include Application type entries
            if desktop_entry.type_() != Some("Application") {
                continue;
            }

            let name = match desktop_entry.name(locales) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let run_cmd = match desktop_entry.exec() {
                Some(e) => {
                    // Clean up run command (remove %u, %U, %f, %F, etc.)
                    e.split_whitespace()
                        .filter(|s| !s.starts_with('%'))
                        .collect::<Vec<_>>()
                        .join(" ")
                }
                None => continue,
            };

            let item = LaunchItem {
                id: path.to_string_lossy().to_string(),
                name,
                description: desktop_entry.comment(locales).map(|s| s.to_string()),
                icon: desktop_entry.icon().map(|s| s.to_string()),
                provider: ProviderType::Application,
                command: run_cmd,
                terminal: desktop_entry.terminal(),
            };

            self.items.push(item);
        }

        debug!("Found {} applications", self.items.len());

        // Sort alphabetically by name
        self.items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}
