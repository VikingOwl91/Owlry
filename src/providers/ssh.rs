use crate::providers::{LaunchItem, Provider, ProviderType};
use log::{debug, warn};
use std::fs;
use std::path::PathBuf;

/// SSH connections provider - parses ~/.ssh/config
pub struct SshProvider {
    items: Vec<LaunchItem>,
    terminal_command: String,
}

impl SshProvider {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_terminal("kitty")
    }

    pub fn with_terminal(terminal: &str) -> Self {
        Self {
            items: Vec::new(),
            terminal_command: terminal.to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn set_terminal(&mut self, terminal: &str) {
        self.terminal_command = terminal.to_string();
    }

    fn ssh_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|p| p.join(".ssh").join("config"))
    }

    fn parse_ssh_config(&mut self) {
        self.items.clear();

        let config_path = match Self::ssh_config_path() {
            Some(p) => p,
            None => {
                debug!("Could not determine SSH config path");
                return;
            }
        };

        if !config_path.exists() {
            debug!("SSH config not found at {:?}", config_path);
            return;
        }

        let content = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read SSH config: {}", e);
                return;
            }
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
            let parts: Vec<&str> = line.splitn(2, |c: char| c.is_whitespace() || c == '=')
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

        debug!("Loaded {} SSH hosts", self.items.len());
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
            icon: Some("utilities-terminal".to_string()),
            provider: ProviderType::Ssh,
            command,
            terminal: false, // We're already wrapping in terminal
        });
    }
}

impl Provider for SshProvider {
    fn name(&self) -> &str {
        "SSH"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Ssh
    }

    fn refresh(&mut self) {
        self.parse_ssh_config();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_config() {
        // This test will only work if the user has an SSH config
        let mut provider = SshProvider::new();
        provider.refresh();
        // Just ensure it doesn't panic
    }
}
