//! Submenu routing helpers.
//!
//! New results carry a typed [`ItemActivation::Submenu`] value. The legacy
//! `SUBMENU:<provider>:<data>` command is decoded only as a compatibility path
//! for older daemons and existing script providers.

use crate::providers::{ItemActivation, LaunchItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmenuRequest {
    pub provider_id: String,
    pub data: String,
}

pub fn request_for_item(item: &LaunchItem) -> Option<SubmenuRequest> {
    let activation = if item.activation.is_launch() {
        ItemActivation::from_legacy_command(&item.command)
    } else {
        item.activation.clone()
    };

    match activation {
        ItemActivation::Submenu { provider_id, data } => Some(SubmenuRequest { provider_id, data }),
        ItemActivation::Launch | ItemActivation::PluginAction { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ItemSource, ProviderType};

    fn item(command: &str, activation: ItemActivation) -> LaunchItem {
        LaunchItem {
            id: "test".into(),
            name: "Test".into(),
            description: None,
            icon: None,
            provider: ProviderType::Plugin("test".into()),
            command: command.into(),
            terminal: false,
            activation,
            tags: Vec::new(),
            source: ItemSource::Core,
        }
    }

    #[test]
    fn typed_submenu_request_is_preferred() {
        let request = request_for_item(&item(
            "ignored",
            ItemActivation::Submenu {
                provider_id: "uuctl".into(),
                data: "demo.service:true".into(),
            },
        ));
        assert_eq!(
            request,
            Some(SubmenuRequest {
                provider_id: "uuctl".into(),
                data: "demo.service:true".into(),
            })
        );
    }

    #[test]
    fn legacy_submenu_command_remains_compatible() {
        let request = request_for_item(&item(
            "SUBMENU:uuctl:demo.service:false",
            ItemActivation::Launch,
        ));
        assert_eq!(
            request,
            Some(SubmenuRequest {
                provider_id: "uuctl".into(),
                data: "demo.service:false".into(),
            })
        );
    }

    #[test]
    fn regular_launch_has_no_submenu() {
        assert_eq!(
            request_for_item(&item("echo hi", ItemActivation::Launch)),
            None
        );
    }
}
