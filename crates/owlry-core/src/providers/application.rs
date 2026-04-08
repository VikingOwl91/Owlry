use std::collections::HashSet;

use super::{ItemSource, LaunchItem, Provider, ProviderType};
use crate::paths;
use freedesktop_desktop_entry::{DesktopEntry, Iter};
use log::{debug, warn};

/// Clean desktop file field codes from command string.
/// Removes %f, %F, %u, %U, %d, %D, %n, %N, %i, %c, %k, %v, %m field codes
/// while preserving quoted arguments and %% (literal percent).
/// See: https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html
fn clean_desktop_exec_field(cmd: &str) -> String {
    let mut result = String::with_capacity(cmd.len());
    let mut chars = cmd.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                result.push(c);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                result.push(c);
            }
            '%' if !in_single_quote => {
                // Check the next character for field code
                if let Some(&next) = chars.peek() {
                    match next {
                        // Standard field codes to remove (with following space if present)
                        'f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'i' | 'c' | 'k' | 'v'
                        | 'm' => {
                            chars.next(); // consume the field code letter
                            // Skip trailing whitespace after the field code
                            while chars.peek() == Some(&' ') {
                                chars.next();
                            }
                        }
                        // %% is escaped percent, output single %
                        '%' => {
                            chars.next();
                            result.push('%');
                        }
                        // Unknown % sequence, keep as-is
                        _ => {
                            result.push('%');
                        }
                    }
                } else {
                    // % at end of string, keep it
                    result.push('%');
                }
            }
            _ => {
                result.push(c);
            }
        }
    }

    // Clean up any double spaces that may have resulted from removing field codes
    let trimmed = result.trim();
    let mut cleaned = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for c in trimmed.chars() {
        if c == ' ' {
            if !prev_space {
                cleaned.push(' ');
            }
            prev_space = true;
        } else {
            cleaned.push(c);
            prev_space = false;
        }
    }

    cleaned
}

#[derive(Default)]
pub struct ApplicationProvider {
    items: Vec<LaunchItem>,
}

impl ApplicationProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_application_dirs() -> Vec<std::path::PathBuf> {
        paths::system_data_dirs()
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

        // Get current desktop environment(s) for OnlyShowIn/NotShowIn filtering
        // XDG_CURRENT_DESKTOP can be colon-separated (e.g., "ubuntu:GNOME")
        let current_desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Track seen .desktop file basenames to skip duplicates.
        // XDG dirs are iterated user-first per spec, so the first occurrence wins.
        let mut seen_basenames: HashSet<String> = HashSet::new();

        for path in Iter::new(dirs.into_iter()) {
            // Skip if we've already loaded a .desktop with this basename from a higher-priority dir.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|basename| !seen_basenames.insert(basename.to_string()))
            {
                debug!("Skipping duplicate desktop entry: {:?}", path);
                continue;
            }

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

            // Apply OnlyShowIn/NotShowIn filters only if we know the current desktop
            // If XDG_CURRENT_DESKTOP is not set, show all apps (don't filter)
            if !current_desktops.is_empty() {
                // OnlyShowIn: if set, current desktop must be in the list
                if desktop_entry.only_show_in().is_some_and(|only| {
                    !current_desktops
                        .iter()
                        .any(|de| only.contains(&de.as_str()))
                }) {
                    continue;
                }

                // NotShowIn: if current desktop is in the list, skip
                if desktop_entry
                    .not_show_in()
                    .is_some_and(|not| current_desktops.iter().any(|de| not.contains(&de.as_str())))
                {
                    continue;
                }
            }

            let name = match desktop_entry.name(locales) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let run_cmd = match desktop_entry.exec() {
                Some(e) => clean_desktop_exec_field(e),
                None => continue,
            };

            // Extract categories and keywords as tags (lowercase for consistency)
            let mut tags: Vec<String> = desktop_entry
                .categories()
                .map(|cats| cats.into_iter().map(|s| s.to_lowercase()).collect())
                .unwrap_or_default();

            // Add keywords for searchability (e.g., Nautilus has Name=Files but Keywords contains "nautilus")
            if let Some(keywords) = desktop_entry.keywords(locales) {
                tags.extend(keywords.into_iter().map(|s| s.to_lowercase()));
            }

            let item = LaunchItem {
                id: path.to_string_lossy().to_string(),
                name,
                description: desktop_entry.comment(locales).map(|s| s.to_string()),
                icon: desktop_entry.icon().map(|s| s.to_string()),
                provider: ProviderType::Application,
                command: run_cmd,
                terminal: desktop_entry.terminal(),
                tags,
                source: ItemSource::Core,
            };

            self.items.push(item);
        }

        debug!("Found {} applications", self.items.len());

        #[cfg(feature = "dev-logging")]
        debug!(
            "XDG_CURRENT_DESKTOP={:?}, scanned dirs count={}",
            current_desktops,
            Self::get_application_dirs().len()
        );

        // Sort alphabetically by name
        self.items
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_desktop_exec_simple() {
        assert_eq!(clean_desktop_exec_field("firefox"), "firefox");
        assert_eq!(clean_desktop_exec_field("firefox %u"), "firefox");
        assert_eq!(clean_desktop_exec_field("code %F"), "code");
    }

    #[test]
    fn test_clean_desktop_exec_multiple_placeholders() {
        assert_eq!(clean_desktop_exec_field("app %f %u %U"), "app");
        assert_eq!(
            clean_desktop_exec_field("app --flag %u --other"),
            "app --flag --other"
        );
    }

    #[test]
    fn test_clean_desktop_exec_preserves_quotes() {
        // Double quotes preserve spacing but field codes are still processed
        assert_eq!(
            clean_desktop_exec_field(r#"bash -c "echo hello""#),
            r#"bash -c "echo hello""#
        );
        // Field codes in double quotes are stripped (per FreeDesktop spec: undefined behavior,
        // but practical implementations strip them)
        assert_eq!(
            clean_desktop_exec_field(r#"bash -c "test %u value""#),
            r#"bash -c "test value""#
        );
    }

    #[test]
    fn test_clean_desktop_exec_escaped_percent() {
        assert_eq!(clean_desktop_exec_field("echo 100%%"), "echo 100%");
    }

    #[test]
    fn test_clean_desktop_exec_single_quotes() {
        assert_eq!(
            clean_desktop_exec_field("bash -c 'echo %u'"),
            "bash -c 'echo %u'"
        );
    }

    #[test]
    fn test_clean_desktop_exec_preserves_env() {
        // env VAR=value pattern should be preserved
        assert_eq!(
            clean_desktop_exec_field("env GDK_BACKEND=x11 UBUNTU_MENUPROXY=0 audacity %F"),
            "env GDK_BACKEND=x11 UBUNTU_MENUPROXY=0 audacity"
        );
        // Multiple env vars
        assert_eq!(
            clean_desktop_exec_field("env FOO=bar BAZ=qux myapp %u"),
            "env FOO=bar BAZ=qux myapp"
        );
    }

    #[test]
    fn test_clean_desktop_exec_collapses_spaces() {
        assert_eq!(clean_desktop_exec_field("app   --flag    arg"), "app --flag arg");
        let input = format!("app{}arg", " ".repeat(100));
        assert_eq!(clean_desktop_exec_field(&input), "app arg");
    }
}
