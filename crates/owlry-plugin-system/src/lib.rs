//! System Plugin for Owlry
//!
//! A static provider that provides system power and session management commands.
//!
//! Commands:
//! - Shutdown - Power off the system
//! - Reboot - Restart the system
//! - Reboot into BIOS - Restart into UEFI/BIOS setup
//! - Suspend - Suspend to RAM
//! - Hibernate - Suspend to disk
//! - Lock Screen - Lock the session
//! - Log Out - End the current session

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind,
    ProviderPosition, API_VERSION,
};

// Plugin metadata
const PLUGIN_ID: &str = "system";
const PLUGIN_NAME: &str = "System";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Power and session management commands";

// Provider metadata
const PROVIDER_ID: &str = "system";
const PROVIDER_NAME: &str = "System";
const PROVIDER_PREFIX: &str = ":sys";
const PROVIDER_ICON: &str = "system-shutdown";
const PROVIDER_TYPE_ID: &str = "system";

/// System provider state - holds cached items
struct SystemState {
    items: Vec<PluginItem>,
}

impl SystemState {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn load_commands(&mut self) {
        self.items.clear();

        // Define system commands
        // Format: (id, name, description, icon, command)
        let commands: &[(&str, &str, &str, &str, &str)] = &[
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
            self.items.push(
                PluginItem::new(*id, *name, *command)
                    .with_description(*description)
                    .with_icon(*icon)
                    .with_keywords(vec!["power".to_string(), "system".to_string()]),
            );
        }
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
        position: ProviderPosition::Normal,
        priority: 0, // Static: use frecency ordering
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(SystemState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<SystemState>
    let state = unsafe { &mut *(handle.ptr as *mut SystemState) };

    // Load/reload commands
    state.load_commands();

    // Return items
    state.items.to_vec().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query is handled by the core using cached items
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<SystemState>
        unsafe {
            handle.drop_as::<SystemState>();
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
    fn test_system_state_new() {
        let state = SystemState::new();
        assert!(state.items.is_empty());
    }

    #[test]
    fn test_system_commands_loaded() {
        let mut state = SystemState::new();
        state.load_commands();

        assert!(state.items.len() >= 6);

        // Check for specific commands
        let names: Vec<&str> = state.items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Shutdown"));
        assert!(names.contains(&"Reboot"));
        assert!(names.contains(&"Suspend"));
        assert!(names.contains(&"Lock Screen"));
        assert!(names.contains(&"Log Out"));
    }

    #[test]
    fn test_reboot_bios_command() {
        let mut state = SystemState::new();
        state.load_commands();

        let bios_cmd = state
            .items
            .iter()
            .find(|i| i.name.as_str() == "Reboot into BIOS")
            .expect("Reboot into BIOS should exist");

        assert_eq!(bios_cmd.command.as_str(), "systemctl reboot --firmware-setup");
    }

    #[test]
    fn test_commands_have_icons() {
        let mut state = SystemState::new();
        state.load_commands();

        for item in &state.items {
            assert!(
                item.icon.is_some(),
                "Item '{}' should have an icon",
                item.name.as_str()
            );
        }
    }

    #[test]
    fn test_commands_have_descriptions() {
        let mut state = SystemState::new();
        state.load_commands();

        for item in &state.items {
            assert!(
                item.description.is_some(),
                "Item '{}' should have a description",
                item.name.as_str()
            );
        }
    }
}
