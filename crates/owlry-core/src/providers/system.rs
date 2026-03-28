use super::{LaunchItem, Provider, ProviderType};

/// Built-in system provider. Returns a fixed set of power and session management actions.
///
/// This is a static provider — items are populated in `new()` and `refresh()` is a no-op.
pub(crate) struct SystemProvider {
    items: Vec<LaunchItem>,
}

impl SystemProvider {
    pub fn new() -> Self {
        let commands: &[(&str, &str, &str, &str, &str)] = &[
            (
                "shutdown",
                "Shutdown",
                "Power off the system",
                "system-shutdown",
                "systemctl poweroff",
            ),
            (
                "reboot",
                "Reboot",
                "Restart the system",
                "system-reboot",
                "systemctl reboot",
            ),
            (
                "reboot-bios",
                "Reboot to BIOS",
                "Restart into UEFI/BIOS setup",
                "system-reboot",
                "systemctl reboot --firmware-setup",
            ),
            (
                "suspend",
                "Suspend",
                "Suspend to RAM",
                "system-suspend",
                "systemctl suspend",
            ),
            (
                "hibernate",
                "Hibernate",
                "Suspend to disk",
                "system-suspend-hibernate",
                "systemctl hibernate",
            ),
            (
                "lock",
                "Lock Screen",
                "Lock the session",
                "system-lock-screen",
                "loginctl lock-session",
            ),
            (
                "logout",
                "Log Out",
                "End the current session",
                "system-log-out",
                "loginctl terminate-session self",
            ),
        ];

        let items = commands
            .iter()
            .map(|(action_id, name, description, icon, command)| LaunchItem {
                id: format!("sys:{}", action_id),
                name: name.to_string(),
                description: Some(description.to_string()),
                icon: Some(icon.to_string()),
                provider: ProviderType::Plugin("sys".into()),
                command: command.to_string(),
                terminal: false,
                tags: vec!["system".into()],
            })
            .collect();

        Self { items }
    }
}

impl Provider for SystemProvider {
    fn name(&self) -> &str {
        "System"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin("sys".into())
    }

    fn refresh(&mut self) {
        // Static provider — no-op
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_seven_actions() {
        let provider = SystemProvider::new();
        assert_eq!(provider.items().len(), 7);
    }

    #[test]
    fn contains_expected_action_names() {
        let provider = SystemProvider::new();
        let names: Vec<&str> = provider.items().iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Shutdown"));
        assert!(names.contains(&"Reboot"));
        assert!(names.contains(&"Lock Screen"));
        assert!(names.contains(&"Log Out"));
    }

    #[test]
    fn provider_type_is_sys_plugin() {
        let provider = SystemProvider::new();
        assert_eq!(provider.provider_type(), ProviderType::Plugin("sys".into()));
    }

    #[test]
    fn shutdown_command_is_correct() {
        let provider = SystemProvider::new();
        let shutdown = provider
            .items()
            .iter()
            .find(|i| i.name == "Shutdown")
            .expect("Shutdown item must exist");
        assert_eq!(shutdown.command, "systemctl poweroff");
    }

    #[test]
    fn all_items_have_system_tag() {
        let provider = SystemProvider::new();
        for item in provider.items() {
            assert!(
                item.tags.contains(&"system".to_string()),
                "item '{}' is missing 'system' tag",
                item.name
            );
        }
    }
}
