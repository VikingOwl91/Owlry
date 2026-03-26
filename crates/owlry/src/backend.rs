//! Abstraction over search backends for the UI.
//!
//! In normal mode, the UI talks to the owlry-core daemon via IPC.
//! In dmenu mode, the UI uses a local ProviderManager directly (no daemon).

use crate::client::CoreClient;
use log::warn;
use owlry_core::config::Config;
use owlry_core::data::FrecencyStore;
use owlry_core::filter::ProviderFilter;
use owlry_core::ipc::ResultItem;
use owlry_core::providers::{LaunchItem, ProviderManager, ProviderType};

/// Backend for search operations. Wraps either an IPC client (daemon mode)
/// or a local ProviderManager (dmenu mode).
pub enum SearchBackend {
    /// IPC client connected to owlry-core daemon
    Daemon(CoreClient),
    /// Direct local provider manager (dmenu mode only)
    Local {
        providers: Box<ProviderManager>,
        frecency: FrecencyStore,
    },
}

impl SearchBackend {
    /// Search for items matching the query.
    ///
    /// In daemon mode, sends query over IPC. The modes list is derived from
    /// the ProviderFilter's enabled set.
    ///
    /// In local mode, delegates to ProviderManager directly.
    pub fn search(
        &mut self,
        query: &str,
        max_results: usize,
        filter: &ProviderFilter,
        config: &Config,
    ) -> Vec<LaunchItem> {
        match self {
            SearchBackend::Daemon(client) => {
                // When accept_all, send None so daemon doesn't restrict to a specific set
                // (otherwise dynamically loaded plugin types would be filtered out)
                let modes_param = if filter.is_accept_all() {
                    None
                } else {
                    let modes: Vec<String> = filter
                        .enabled_providers()
                        .iter()
                        .map(|p| p.to_string())
                        .collect();
                    if modes.is_empty() { None } else { Some(modes) }
                };

                match client.query(query, modes_param) {
                    Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                    Err(e) => {
                        warn!("IPC query failed: {}", e);
                        Vec::new()
                    }
                }
            }
            SearchBackend::Local {
                providers,
                frecency,
            } => {
                let frecency_weight = config.providers.frecency_weight;
                let use_frecency = config.providers.frecency;

                if use_frecency {
                    providers
                        .search_with_frecency(
                            query,
                            max_results,
                            filter,
                            frecency,
                            frecency_weight,
                            None,
                        )
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                } else {
                    providers
                        .search_filtered(query, max_results, filter)
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                }
            }
        }
    }

    /// Search with tag filter support.
    pub fn search_with_tag(
        &mut self,
        query: &str,
        max_results: usize,
        filter: &ProviderFilter,
        config: &Config,
        tag_filter: Option<&str>,
    ) -> Vec<LaunchItem> {
        match self {
            SearchBackend::Daemon(client) => {
                // Daemon doesn't support tag filtering in IPC yet — pass query as-is.
                // If there's a tag filter, prepend it so the daemon can handle it.
                let effective_query = if let Some(tag) = tag_filter {
                    format!(":tag:{} {}", tag, query)
                } else {
                    query.to_string()
                };

                // When accept_all, send None so daemon doesn't restrict to a specific set
                // (otherwise dynamically loaded plugin types would be filtered out)
                let modes_param = if filter.is_accept_all() {
                    None
                } else {
                    let modes: Vec<String> = filter
                        .enabled_providers()
                        .iter()
                        .map(|p| p.to_string())
                        .collect();
                    if modes.is_empty() { None } else { Some(modes) }
                };

                match client.query(&effective_query, modes_param) {
                    Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                    Err(e) => {
                        warn!("IPC query failed: {}", e);
                        Vec::new()
                    }
                }
            }
            SearchBackend::Local {
                providers,
                frecency,
            } => {
                let frecency_weight = config.providers.frecency_weight;
                let use_frecency = config.providers.frecency;

                if use_frecency {
                    providers
                        .search_with_frecency(
                            query,
                            max_results,
                            filter,
                            frecency,
                            frecency_weight,
                            tag_filter,
                        )
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                } else {
                    providers
                        .search_filtered(query, max_results, filter)
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                }
            }
        }
    }

    /// Execute a plugin action command. Returns true if handled.
    pub fn execute_plugin_action(&mut self, command: &str) -> bool {
        match self {
            SearchBackend::Daemon(client) => match client.plugin_action(command) {
                Ok(handled) => handled,
                Err(e) => {
                    warn!("IPC plugin_action failed: {}", e);
                    false
                }
            },
            SearchBackend::Local { providers, .. } => providers.execute_plugin_action(command),
        }
    }

    /// Query submenu actions for a plugin item.
    /// Returns (display_name, actions) if available.
    pub fn query_submenu_actions(
        &mut self,
        plugin_id: &str,
        data: &str,
        display_name: &str,
    ) -> Option<(String, Vec<LaunchItem>)> {
        match self {
            SearchBackend::Daemon(client) => match client.submenu(plugin_id, data) {
                Ok(items) if !items.is_empty() => {
                    let actions: Vec<LaunchItem> =
                        items.into_iter().map(result_to_launch_item).collect();
                    Some((display_name.to_string(), actions))
                }
                Ok(_) => None,
                Err(e) => {
                    warn!("IPC submenu query failed: {}", e);
                    None
                }
            },
            SearchBackend::Local { providers, .. } => {
                providers.query_submenu_actions(plugin_id, data, display_name)
            }
        }
    }

    /// Record a launch event for frecency tracking.
    pub fn record_launch(&mut self, item_id: &str, provider: &str) {
        match self {
            SearchBackend::Daemon(client) => {
                if let Err(e) = client.launch(item_id, provider) {
                    warn!("IPC launch notification failed: {}", e);
                }
            }
            SearchBackend::Local { frecency, .. } => {
                frecency.record_launch(item_id);
            }
        }
    }

    /// Whether this backend is in dmenu mode.
    pub fn is_dmenu_mode(&self) -> bool {
        match self {
            SearchBackend::Daemon(_) => false,
            SearchBackend::Local { providers, .. } => providers.is_dmenu_mode(),
        }
    }

    /// Refresh widget providers. No-op for daemon mode (daemon handles refresh).
    pub fn refresh_widgets(&mut self) {
        if let SearchBackend::Local { providers, .. } = self {
            providers.refresh_widgets();
        }
    }

    /// Get available provider type IDs from the daemon, or from local manager.
    #[allow(dead_code)]
    pub fn available_provider_ids(&mut self) -> Vec<String> {
        match self {
            SearchBackend::Daemon(client) => match client.providers() {
                Ok(descs) => descs.into_iter().map(|d| d.id).collect(),
                Err(e) => {
                    warn!("IPC providers query failed: {}", e);
                    Vec::new()
                }
            },
            SearchBackend::Local { providers, .. } => providers
                .available_providers()
                .into_iter()
                .map(|d| d.id)
                .collect(),
        }
    }
}

/// Convert an IPC ResultItem to the internal LaunchItem type.
fn result_to_launch_item(item: ResultItem) -> LaunchItem {
    let provider: ProviderType = item.provider.parse().unwrap_or(ProviderType::Application);
    LaunchItem {
        id: item.id,
        name: item.title,
        description: if item.description.is_empty() {
            None
        } else {
            Some(item.description)
        },
        icon: if item.icon.is_empty() {
            None
        } else {
            Some(item.icon)
        },
        provider,
        command: item.command.unwrap_or_default(),
        terminal: item.terminal,
        tags: item.tags,
    }
}
