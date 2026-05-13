//! SSH hosts provider.
//!
//! Parses `~/.ssh/config` and exposes each non-wildcard `Host` entry as a
//! launchable item that opens an `ssh <host>` session in the user's terminal.

use super::{ItemSource, LaunchItem, Provider, ProviderType};
use std::fs;
use std::path::PathBuf;

const TYPE_ID: &str = "ssh";
const ICON: &str = "utilities-terminal";

pub struct SshProvider {
    items: Vec<LaunchItem>,
    terminal_command: String,
}

impl Default for SshProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SshProvider {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            terminal_command: Self::load_terminal_from_config(),
        }
    }

    fn load_terminal_from_config() -> String {
        // Try [plugins.ssh] in config.toml
        let config_path = dirs::config_dir().map(|d| d.join("owlry").join("config.toml"));
        if let Some(content) = config_path.and_then(|p| fs::read_to_string(p).ok())
            && let Ok(toml) = content.parse::<toml::Table>()
            && let Some(plugins) = toml.get("plugins").and_then(|v| v.as_table())
            && let Some(ssh) = plugins.get("ssh").and_then(|v| v.as_table())
            && let Some(terminal) = ssh.get("terminal").and_then(|v| v.as_str())
        {
            return terminal.to_string();
        }

        // Fall back to $TERMINAL env var
        if let Ok(terminal) = std::env::var("TERMINAL") {
            return terminal;
        }

        // Last resort
        "xdg-terminal-exec".to_string()
    }

    fn ssh_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".ssh").join("config"))
    }

    fn parse_ssh_config(&mut self) {
        self.items.clear();

        let config_path = match Self::ssh_config_path() {
            Some(p) => p,
            None => return,
        };

        if !config_path.exists() {
            return;
        }

        let content = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut current_host: Option<String> = None;
        let mut current_hostname: Option<String> = None;
        let mut current_user: Option<String> = None;
        let mut current_port: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Split on whitespace or '='
            let parts: Vec<&str> = line
                .splitn(2, |c: char| c.is_whitespace() || c == '=')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if parts.len() < 2 {
                continue;
            }

            let key = parts[0].to_lowercase();
            let value = parts[1];

            match key.as_str() {
                "host" => {
                    // Save previous host if exists
                    if let Some(host) = current_host.take() {
                        self.add_host_item(
                            &host,
                            current_hostname.take(),
                            current_user.take(),
                            current_port.take(),
                        );
                    }

                    // Skip wildcards and patterns
                    if !value.contains('*') && !value.contains('?') && value != "*" {
                        current_host = Some(value.to_string());
                    }
                    current_hostname = None;
                    current_user = None;
                    current_port = None;
                }
                "hostname" => {
                    current_hostname = Some(value.to_string());
                }
                "user" => {
                    current_user = Some(value.to_string());
                }
                "port" => {
                    current_port = Some(value.to_string());
                }
                _ => {}
            }
        }

        // Don't forget the last host
        if let Some(host) = current_host.take() {
            self.add_host_item(&host, current_hostname, current_user, current_port);
        }
    }

    fn add_host_item(
        &mut self,
        host: &str,
        hostname: Option<String>,
        user: Option<String>,
        port: Option<String>,
    ) {
        // Build description
        let mut desc_parts = Vec::new();
        if let Some(ref h) = hostname {
            desc_parts.push(h.clone());
        }
        if let Some(ref u) = user {
            desc_parts.push(format!("user: {}", u));
        }
        if let Some(ref p) = port {
            desc_parts.push(format!("port: {}", p));
        }

        let description = if desc_parts.is_empty() {
            None
        } else {
            Some(desc_parts.join(", "))
        };

        // Build SSH command - just use the host alias, SSH will resolve the rest
        let ssh_command = format!("ssh {}", host);

        // Wrap in terminal
        let command = format!("{} -e {}", self.terminal_command, ssh_command);

        self.items.push(LaunchItem {
            id: format!("ssh:{}", host),
            name: format!("SSH: {}", host),
            description,
            icon: Some(ICON.to_string()),
            provider: ProviderType::Plugin(TYPE_ID.into()),
            command,
            terminal: false,
            tags: vec!["ssh".to_string(), "remote".to_string()],
            source: ItemSource::Core,
        });
    }
}

impl Provider for SshProvider {
    fn name(&self) -> &str {
        "SSH"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        self.parse_ssh_config();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn prefix(&self) -> Option<&str> {
        Some(":ssh")
    }

    fn icon(&self) -> &str {
        ICON
    }

    fn tab_label(&self) -> Option<&str> {
        Some("SSH")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("SSH hosts")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_provider_new() {
        let p = SshProvider::new();
        assert!(p.items.is_empty());
    }

    #[test]
    fn test_parse_simple_config() {
        let mut p = SshProvider::new();

        // We can't easily test the full flow without mocking file paths,
        // but we can test the add_host_item method
        p.add_host_item(
            "myserver",
            Some("192.168.1.100".to_string()),
            Some("admin".to_string()),
            Some("2222".to_string()),
        );

        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].name, "SSH: myserver");
        assert!(p.items[0].command.contains("ssh myserver"));
    }

    #[test]
    fn test_add_host_without_details() {
        let mut p = SshProvider::new();
        p.add_host_item("simple-host", None, None, None);

        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].name, "SSH: simple-host");
        assert!(p.items[0].description.is_none());
    }

    #[test]
    fn test_add_host_with_partial_details() {
        let mut p = SshProvider::new();
        p.add_host_item("partial", Some("example.com".to_string()), None, None);

        assert_eq!(p.items.len(), 1);
        let desc = p.items[0].description.as_ref().unwrap();
        assert_eq!(desc, "example.com");
    }

    #[test]
    fn test_items_have_icons() {
        let mut p = SshProvider::new();
        p.add_host_item("test", None, None, None);

        assert!(p.items[0].icon.is_some());
        assert_eq!(p.items[0].icon.as_ref().unwrap(), ICON);
    }

    #[test]
    fn test_items_have_keywords() {
        let mut p = SshProvider::new();
        p.add_host_item("test", None, None, None);

        assert!(!p.items[0].tags.is_empty());
        assert!(p.items[0].tags.iter().any(|t| t == "ssh"));
    }

    #[test]
    fn provider_type_is_ssh_plugin() {
        let p = SshProvider::new();
        assert_eq!(p.provider_type(), ProviderType::Plugin("ssh".into()));
    }
}
