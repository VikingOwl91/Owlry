//! Abstraction over search backends for the UI.
//!
//! In normal mode, the UI talks to the owlry-core daemon via IPC.
//! In dmenu mode, the UI uses a local ProviderManager directly (no daemon).

use crate::client::CoreClient;
use crate::config::Config;
use crate::data::FrecencyStore;
use crate::filter::ProviderFilter;
use crate::ipc::{ProviderDesc, ResultItem};
use crate::providers::{ItemSource, LaunchItem, ProviderManager, ProviderType};
use log::warn;
use std::sync::{Arc, Mutex};

/// Parameters needed to run a search query on a background thread.
pub struct QueryParams {
    pub query: String,
    #[allow(dead_code)]
    pub max_results: usize,
    pub modes: Option<Vec<String>>,
    pub tag_filter: Option<String>,
}

/// Result of an async search, sent back to the main thread.
pub struct QueryResult {
    #[allow(dead_code)]
    pub query: String,
    pub items: Vec<LaunchItem>,
}

/// Thread-safe handle to the daemon IPC connection.
pub struct DaemonHandle {
    pub(crate) client: Arc<Mutex<CoreClient>>,
}

impl DaemonHandle {
    pub fn new(client: CoreClient) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
        }
    }

    /// Dispatch an IPC query on a background thread.
    ///
    /// Returns a `futures_channel::oneshot::Receiver` that resolves with
    /// the `QueryResult` once the background thread completes IPC. The
    /// caller should `.await` it inside `glib::spawn_future_local` to
    /// process results on the GTK main thread without `Send` constraints.
    pub fn query_async(
        &self,
        params: QueryParams,
    ) -> futures_channel::oneshot::Receiver<QueryResult> {
        let (tx, rx) = futures_channel::oneshot::channel();
        let client = Arc::clone(&self.client);
        let query_for_result = params.query.clone();

        std::thread::spawn(move || {
            let items = match client.lock() {
                Ok(mut c) => {
                    let effective_query = if let Some(ref tag) = params.tag_filter {
                        format!(":tag:{} {}", tag, params.query)
                    } else {
                        params.query
                    };
                    match c.query(&effective_query, params.modes) {
                        Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                        Err(e) => {
                            warn!("IPC query failed: {}", e);
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to lock daemon client: {}", e);
                    Vec::new()
                }
            };

            let _ = tx.send(QueryResult {
                query: query_for_result,
                items,
            });
        });

        rx
    }
}

/// Backend for search operations. Wraps either an IPC client (daemon mode)
/// or a local ProviderManager (dmenu mode).
pub enum SearchBackend {
    /// IPC client connected to owlry-core daemon
    Daemon(DaemonHandle),
    /// Direct local provider manager (dmenu mode only)
    Local {
        providers: Box<ProviderManager>,
        frecency: FrecencyStore,
    },
}

impl SearchBackend {
    /// Build the modes parameter from a ProviderFilter.
    /// When accept_all, returns None so the daemon doesn't restrict to a specific set
    /// (otherwise dynamically loaded plugin types would be filtered out).
    fn build_modes_param(filter: &ProviderFilter) -> Option<Vec<String>> {
        if filter.is_accept_all() {
            None
        } else {
            let modes: Vec<String> = filter
                .enabled_providers()
                .iter()
                .map(|p| p.to_string())
                .collect();
            if modes.is_empty() { None } else { Some(modes) }
        }
    }

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
            SearchBackend::Daemon(handle) => {
                let modes_param = Self::build_modes_param(filter);
                match handle.client.lock() {
                    Ok(mut client) => match client.query(query, modes_param) {
                        Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                        Err(e) => {
                            warn!("IPC query failed: {}", e);
                            Vec::new()
                        }
                    },
                    Err(e) => {
                        warn!("Failed to lock daemon client: {}", e);
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
                        .search_filtered(query, max_results, filter, None)
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
            SearchBackend::Daemon(handle) => {
                let effective_query = if let Some(tag) = tag_filter {
                    format!(":tag:{} {}", tag, query)
                } else {
                    query.to_string()
                };

                let modes_param = Self::build_modes_param(filter);
                match handle.client.lock() {
                    Ok(mut client) => match client.query(&effective_query, modes_param) {
                        Ok(items) => items.into_iter().map(result_to_launch_item).collect(),
                        Err(e) => {
                            warn!("IPC query failed: {}", e);
                            Vec::new()
                        }
                    },
                    Err(e) => {
                        warn!("Failed to lock daemon client: {}", e);
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
                        .search_filtered(query, max_results, filter, tag_filter)
                        .into_iter()
                        .map(|(item, _)| item)
                        .collect()
                }
            }
        }
    }

    /// Dispatch async search (daemon mode only).
    /// Returns `Some(Receiver)` if dispatched, `None` for local mode.
    pub fn query_async(
        &self,
        query: &str,
        max_results: usize,
        filter: &ProviderFilter,
        _config: &Config,
        tag_filter: Option<&str>,
    ) -> Option<futures_channel::oneshot::Receiver<QueryResult>> {
        match self {
            SearchBackend::Daemon(handle) => {
                let params = QueryParams {
                    query: query.to_string(),
                    max_results,
                    modes: Self::build_modes_param(filter),
                    tag_filter: tag_filter.map(|s| s.to_string()),
                };
                Some(handle.query_async(params))
            }
            SearchBackend::Local { .. } => None,
        }
    }

    /// Execute a plugin action command. Returns true if handled.
    pub fn execute_plugin_action(&mut self, command: &str) -> bool {
        match self {
            SearchBackend::Daemon(handle) => match handle.client.lock() {
                Ok(mut client) => match client.plugin_action(command) {
                    Ok(handled) => handled,
                    Err(e) => {
                        warn!("IPC plugin_action failed: {}", e);
                        false
                    }
                },
                Err(e) => {
                    warn!("Failed to lock daemon client: {}", e);
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
            SearchBackend::Daemon(handle) => match handle.client.lock() {
                Ok(mut client) => match client.submenu(plugin_id, data) {
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
                Err(e) => {
                    warn!("Failed to lock daemon client: {}", e);
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
            SearchBackend::Daemon(handle) => {
                if let Ok(mut client) = handle.client.lock() {
                    if let Err(e) = client.launch(item_id, provider) {
                        warn!("IPC launch notification failed: {}", e);
                    }
                } else {
                    warn!("Failed to lock daemon client for launch");
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

    /// Refresh widget providers. No-op in Phase 1 (widgets deferred per D20).
    /// Retained as a stable no-op so callers don't need conditional compilation.
    pub fn refresh_widgets(&mut self) {}

    /// Get available provider descriptors from the daemon, or from local manager.
    pub fn available_providers(&mut self) -> Vec<ProviderDesc> {
        match self {
            SearchBackend::Daemon(handle) => match handle.client.lock() {
                Ok(mut client) => match client.providers() {
                    Ok(descs) => descs,
                    Err(e) => {
                        warn!("IPC providers query failed: {}", e);
                        Vec::new()
                    }
                },
                Err(e) => {
                    warn!("Failed to lock daemon client: {}", e);
                    Vec::new()
                }
            },
            SearchBackend::Local { providers, .. } => providers
                .available_providers()
                .into_iter()
                .map(|d| ProviderDesc {
                    id: d.id,
                    name: d.name,
                    prefix: d.prefix,
                    icon: d.icon,
                    position: d.position,
                    tab_label: d.tab_label,
                    search_noun: d.search_noun,
                })
                .collect(),
        }
    }
}

/// Convert an IPC ResultItem to the internal LaunchItem type.
fn result_to_launch_item(item: ResultItem) -> LaunchItem {
    let provider: ProviderType = item.provider.parse().unwrap_or(ProviderType::Application);
    let source: ItemSource = item.source.parse().unwrap_or(ItemSource::Core);
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
        source,
    }
}
