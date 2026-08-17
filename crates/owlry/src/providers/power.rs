use super::{ItemSource, LaunchItem, Provider, ProviderType};

/// Built-in power & session provider. Returns a fixed set of shutdown / reboot /
/// suspend / lock / logout actions.
///
/// Static provider — items are populated in `new()` and `refresh()` is a no-op.
///
/// type_id is `"power"`. CLI aliases `:sys` and `:system` still resolve here for
/// muscle-memory back-compat (see [`crate::providers::ProviderType::FromStr`]
/// and [`crate::filter::ProviderFilter::parse_query`]).
pub(crate) struct PowerProvider {
    items: Vec<LaunchItem>,
}

const TYPE_ID: &str = "power";

impl PowerProvider {
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
                id: format!("power:{}", action_id),
                name: name.to_string(),
                description: Some(description.to_string()),
                icon: Some(icon.to_string()),
                provider: ProviderType::Plugin(TYPE_ID.into()),
                command: command.to_string(),
                terminal: false,
                activation: super::ItemActivation::Launch,
                tags: vec!["power".into()],
                source: ItemSource::Core,
            })
            .collect();

        Self { items }
    }
}

impl Provider for PowerProvider {
    fn name(&self) -> &str {
        "Power"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin(TYPE_ID.into())
    }

    fn refresh(&mut self) {
        // Static provider — no-op
    }

    fn items(&self) -> &[LaunchItem] {
        &self.items
    }

    fn prefix(&self) -> Option<&str> {
        Some(":power")
    }

    fn icon(&self) -> &str {
        "system-shutdown"
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Power")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("power actions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_seven_actions() {
        let provider = PowerProvider::new();
        assert_eq!(provider.items().len(), 7);
    }

    #[test]
    fn contains_expected_action_names() {
        let provider = PowerProvider::new();
        let names: Vec<&str> = provider.items().iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Shutdown"));
        assert!(names.contains(&"Reboot"));
        assert!(names.contains(&"Lock Screen"));
        assert!(names.contains(&"Log Out"));
    }

    #[test]
    fn provider_type_is_power_plugin() {
        let provider = PowerProvider::new();
        assert_eq!(
            provider.provider_type(),
            ProviderType::Plugin("power".into())
        );
    }

    #[test]
    fn shutdown_command_is_correct() {
        let provider = PowerProvider::new();
        let shutdown = provider
            .items()
            .iter()
            .find(|i| i.name == "Shutdown")
            .expect("Shutdown item must exist");
        assert_eq!(shutdown.command, "systemctl poweroff");
    }

    #[test]
    fn all_items_have_power_tag() {
        let provider = PowerProvider::new();
        for item in provider.items() {
            assert!(
                item.tags.contains(&"power".to_string()),
                "item '{}' is missing 'power' tag",
                item.name
            );
        }
    }

    #[test]
    fn all_item_ids_use_power_prefix() {
        // Frecency / IPC compatibility check: every item id must start with
        // the canonical "power:" prefix after the v2 rename.
        let provider = PowerProvider::new();
        for item in provider.items() {
            assert!(
                item.id.starts_with("power:"),
                "item id '{}' should start with 'power:'",
                item.id
            );
        }
    }
}
