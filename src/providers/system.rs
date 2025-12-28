use crate::providers::{LaunchItem, Provider, ProviderType};

/// System commands provider - shutdown, reboot, lock, etc.
pub struct SystemProvider {
    items: Vec<LaunchItem>,
}

impl SystemProvider {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn load_commands(&mut self) {
        self.items.clear();

        // Define system commands
        // Format: (id, name, description, icon, command)
        let commands: Vec<(&str, &str, &str, &str, &str)> = vec![
            (
                "system:shutdown",
                "Shutdown",
                "Power off the system",
                "system-shutdown",
                "systemctl poweroff",
            ),
            (
                "system:reboot",
                "Reboot",
                "Restart the system",
                "system-reboot",
                "systemctl reboot",
            ),
            (
                "system:reboot-bios",
                "Reboot into BIOS",
                "Restart into UEFI/BIOS setup",
                "system-reboot",
                "systemctl reboot --firmware-setup",
            ),
            (
                "system:suspend",
                "Suspend",
                "Suspend to RAM",
                "system-suspend",
                "systemctl suspend",
            ),
            (
                "system:hibernate",
                "Hibernate",
                "Suspend to disk",
                "system-suspend-hibernate",
                "systemctl hibernate",
            ),
            (
                "system:lock",
                "Lock Screen",
                "Lock the session",
                "system-lock-screen",
                "loginctl lock-session",
            ),
            (
                "system:logout",
                "Log Out",
                "End the current session",
                "system-log-out",
                "loginctl terminate-session self",
            ),
        ];

        for (id, name, description, icon, command) in commands {
            self.items.push(LaunchItem {
                id: id.to_string(),
                name: name.to_string(),
                description: Some(description.to_string()),
                icon: Some(icon.to_string()),
                provider: ProviderType::System,
                command: command.to_string(),
                terminal: false,
            });
        }
    }
}

impl Provider for SystemProvider {
    fn name(&self) -> &str {
        "System"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::System
    }

    fn refresh(&mut self) {
        self.load_commands();
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_provider() {
        let mut provider = SystemProvider::new();
        provider.refresh();

        assert!(provider.items().len() >= 6);
        assert!(provider.items().iter().any(|i| i.name == "Shutdown"));
        assert!(provider.items().iter().any(|i| i.name == "Reboot into BIOS"));
    }
}
