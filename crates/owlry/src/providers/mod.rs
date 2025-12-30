// Core providers (no plugin equivalents)
mod application;
mod command;
mod dmenu;

// Native plugin bridge
pub mod native_provider;

// Lua plugin bridge (optional)
#[cfg(feature = "lua")]
pub mod lua_provider;

// Re-exports for core providers
pub use application::ApplicationProvider;
pub use command::CommandProvider;
pub use dmenu::DmenuProvider;

// Re-export native provider for plugin loading
pub use native_provider::NativeProvider;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use log::info;

#[cfg(feature = "dev-logging")]
use log::debug;

use crate::data::FrecencyStore;

/// Represents a single searchable/launchable item
#[derive(Debug, Clone)]
pub struct LaunchItem {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub provider: ProviderType,
    pub command: String,
    pub terminal: bool,
    /// Tags/categories for filtering (e.g., from .desktop Categories)
    pub tags: Vec<String>,
}

/// Provider type identifier for filtering and badge display
///
/// Core types are built-in providers. All native plugins use Plugin(type_id).
/// This keeps the core app free of plugin-specific knowledge.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderType {
    /// Built-in: Desktop applications from XDG directories
    Application,
    /// Built-in: Shell commands from PATH
    Command,
    /// Built-in: Pipe-based input (dmenu compatibility)
    Dmenu,
    /// Plugin-defined provider type with its type_id (e.g., "calc", "weather", "emoji")
    Plugin(String),
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // Core built-in providers
            "app" | "apps" | "application" | "applications" => Ok(ProviderType::Application),
            "cmd" | "command" | "commands" => Ok(ProviderType::Command),
            "dmenu" => Ok(ProviderType::Dmenu),
            // Everything else is a plugin
            other => Ok(ProviderType::Plugin(other.to_string())),
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Application => write!(f, "app"),
            ProviderType::Command => write!(f, "cmd"),
            ProviderType::Dmenu => write!(f, "dmenu"),
            ProviderType::Plugin(type_id) => write!(f, "{}", type_id),
        }
    }
}

/// Trait for all search providers
pub trait Provider: Send {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn refresh(&mut self);
    fn items(&self) -> &[LaunchItem];
}

/// Manages all providers and handles searching
pub struct ProviderManager {
    /// Static providers (apps, commands, and native static plugins)
    providers: Vec<Box<dyn Provider>>,
    /// Dynamic providers from native plugins (calculator, websearch, filesearch)
    /// These are queried per-keystroke, not cached
    dynamic_providers: Vec<NativeProvider>,
    /// Widget providers from native plugins (weather, media, pomodoro)
    /// These appear at the top of results
    widget_providers: Vec<NativeProvider>,
    /// Fuzzy matcher for search
    matcher: SkimMatcherV2,
}

impl ProviderManager {
    /// Create a new ProviderManager with native plugins
    ///
    /// Native plugins are loaded from /usr/lib/owlry/plugins/ and categorized based on
    /// their declared ProviderKind and ProviderPosition:
    /// - Static providers with Normal position (added to providers vec)
    /// - Dynamic providers (queried per-keystroke, declared via ProviderKind::Dynamic)
    /// - Widget providers (shown at top, declared via ProviderPosition::Widget)
    pub fn with_native_plugins(native_providers: Vec<NativeProvider>) -> Self {
        let mut manager = Self {
            providers: Vec::new(),
            dynamic_providers: Vec::new(),
            widget_providers: Vec::new(),
            matcher: SkimMatcherV2::default(),
        };

        // Check if running in dmenu mode (stdin has data)
        let dmenu_mode = DmenuProvider::has_stdin_data();

        if dmenu_mode {
            // In dmenu mode, only use dmenu provider
            let mut dmenu = DmenuProvider::new();
            dmenu.enable();
            manager.providers.push(Box::new(dmenu));
        } else {
            // Core providers (no plugin equivalents)
            manager.providers.push(Box::new(ApplicationProvider::new()));
            manager.providers.push(Box::new(CommandProvider::new()));

            // Categorize native plugins based on their declared ProviderKind and ProviderPosition
            for provider in native_providers {
                let type_id = provider.type_id();

                if provider.is_dynamic() {
                    // Dynamic providers declare ProviderKind::Dynamic
                    info!("Registered dynamic provider: {} ({})", provider.name(), type_id);
                    manager.dynamic_providers.push(provider);
                } else if provider.is_widget() {
                    // Widgets declare ProviderPosition::Widget
                    info!("Registered widget provider: {} ({})", provider.name(), type_id);
                    manager.widget_providers.push(provider);
                } else {
                    // Static providers with Normal position
                    info!("Registered static provider: {} ({})", provider.name(), type_id);
                    manager.providers.push(Box::new(provider));
                }
            }
        }

        // Initial refresh
        manager.refresh_all();

        manager
    }

    #[allow(dead_code)]
    pub fn is_dmenu_mode(&self) -> bool {
        self.providers
            .iter()
            .any(|p| p.provider_type() == ProviderType::Dmenu)
    }

    pub fn refresh_all(&mut self) {
        // Refresh static providers (fast, local operations)
        for provider in &mut self.providers {
            provider.refresh();
            info!(
                "Provider '{}' loaded {} items",
                provider.name(),
                provider.items().len()
            );
        }

        // Widget providers are refreshed separately to avoid blocking startup
        // Call refresh_widgets() after window is shown

        // Dynamic providers don't need refresh (they query on demand)
    }

    /// Refresh widget providers (weather, media, pomodoro)
    /// Call this separately from refresh_all() to avoid blocking startup
    /// since widgets may make network requests or spawn processes
    pub fn refresh_widgets(&mut self) {
        for provider in &mut self.widget_providers {
            provider.refresh();
            info!(
                "Widget '{}' loaded {} items",
                provider.name(),
                provider.items().len()
            );
        }
    }

    /// Find a native provider by type ID
    /// Searches in widget providers and dynamic providers
    pub fn find_native_provider(&self, type_id: &str) -> Option<&NativeProvider> {
        // Check widget providers first (pomodoro, weather, media)
        if let Some(p) = self.widget_providers.iter().find(|p| p.type_id() == type_id) {
            return Some(p);
        }
        // Then dynamic providers (calc, websearch, filesearch)
        self.dynamic_providers.iter().find(|p| p.type_id() == type_id)
    }

    /// Execute a plugin action command
    /// Command format: PLUGIN_ID:action_data (e.g., "POMODORO:start", "SYSTEMD:unit:restart")
    /// Returns true if the command was handled by a plugin
    pub fn execute_plugin_action(&self, command: &str) -> bool {
        // Parse command format: PLUGIN_ID:action_data
        if let Some(colon_pos) = command.find(':') {
            let plugin_id = &command[..colon_pos];
            let action = command; // Pass full command to plugin

            // Find provider by type ID (case-insensitive for convenience)
            let type_id = plugin_id.to_lowercase();

            if let Some(provider) = self.find_native_provider(&type_id) {
                provider.execute_action(action);
                return true;
            }
        }
        false
    }

    /// Add a dynamic provider (e.g., from a Lua plugin)
    #[allow(dead_code)]
    pub fn add_provider(&mut self, provider: Box<dyn Provider>) {
        info!("Added plugin provider: {}", provider.name());
        self.providers.push(provider);
    }

    /// Add multiple providers at once (for batch plugin loading)
    #[allow(dead_code)]
    pub fn add_providers(&mut self, providers: Vec<Box<dyn Provider>>) {
        for provider in providers {
            self.add_provider(provider);
        }
    }

    #[allow(dead_code)]
    pub fn search(&self, query: &str, max_results: usize) -> Vec<(LaunchItem, i64)> {
        if query.is_empty() {
            // Return recent/popular items when query is empty
            return self.providers
                .iter()
                .flat_map(|p| p.items().iter().cloned())
                .take(max_results)
                .map(|item| (item, 0))
                .collect();
        }

        let mut results: Vec<(LaunchItem, i64)> = self.providers
            .iter()
            .flat_map(|provider| {
                provider.items().iter().filter_map(|item| {
                    // Match against name and description
                    let name_score = self.matcher.fuzzy_match(&item.name, query);
                    let desc_score = item.description
                        .as_ref()
                        .and_then(|d| self.matcher.fuzzy_match(d, query));

                    let score = match (name_score, desc_score) {
                        (Some(n), Some(d)) => Some(n.max(d)),
                        (Some(n), None) => Some(n),
                        (None, Some(d)) => Some(d / 2), // Lower weight for description matches
                        (None, None) => None,
                    };

                    score.map(|s| (item.clone(), s))
                })
            })
            .collect();

        // Sort by score (descending)
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(max_results);
        results
    }

    /// Search with provider filtering
    pub fn search_filtered(
        &self,
        query: &str,
        max_results: usize,
        filter: &crate::filter::ProviderFilter,
    ) -> Vec<(LaunchItem, i64)> {
        if query.is_empty() {
            return self
                .providers
                .iter()
                .filter(|p| filter.is_active(p.provider_type()))
                .flat_map(|p| p.items().iter().cloned())
                .take(max_results)
                .map(|item| (item, 0))
                .collect();
        }

        let mut results: Vec<(LaunchItem, i64)> = self
            .providers
            .iter()
            .filter(|provider| filter.is_active(provider.provider_type()))
            .flat_map(|provider| {
                provider.items().iter().filter_map(|item| {
                    let name_score = self.matcher.fuzzy_match(&item.name, query);
                    let desc_score = item
                        .description
                        .as_ref()
                        .and_then(|d| self.matcher.fuzzy_match(d, query));

                    let score = match (name_score, desc_score) {
                        (Some(n), Some(d)) => Some(n.max(d)),
                        (Some(n), None) => Some(n),
                        (None, Some(d)) => Some(d / 2),
                        (None, None) => None,
                    };

                    score.map(|s| (item.clone(), s))
                })
            })
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(max_results);
        results
    }

    /// Search with frecency boosting, dynamic providers, and tag filtering
    pub fn search_with_frecency(
        &self,
        query: &str,
        max_results: usize,
        filter: &crate::filter::ProviderFilter,
        frecency: &FrecencyStore,
        frecency_weight: f64,
        tag_filter: Option<&str>,
    ) -> Vec<(LaunchItem, i64)> {
        #[cfg(feature = "dev-logging")]
        debug!("[Search] query={:?}, max={}, frecency_weight={}", query, max_results, frecency_weight);

        let mut results: Vec<(LaunchItem, i64)> = Vec::new();

        // Add widget items first (highest priority) - only when:
        // 1. No specific filter prefix is active
        // 2. Query is empty (user hasn't started searching)
        // This keeps widgets visible on launch but hides them during active search
        // Widgets are always visible regardless of filter settings (they declare position via API)
        if filter.active_prefix().is_none() && query.is_empty() {
            // Widget priority comes from plugin-declared priority field
            for provider in &self.widget_providers {
                let base_score = provider.priority() as i64;
                for (idx, item) in provider.items().iter().enumerate() {
                    results.push((item.clone(), base_score - idx as i64));
                }
            }
        }

        // Query dynamic providers (calculator, websearch, filesearch)
        // Only query if:
        // 1. Their specific filter is active (e.g., :file prefix or Files tab selected), OR
        // 2. No specific single-mode filter is active (showing all providers)
        if !query.is_empty() {
            for provider in &self.dynamic_providers {
                // Skip if this provider type is explicitly filtered out
                if !filter.is_active(provider.provider_type()) {
                    continue;
                }
                let dynamic_results = provider.query(query);
                // Priority comes from plugin-declared priority field
                let base_score = provider.priority() as i64;
                for (idx, item) in dynamic_results.into_iter().enumerate() {
                    results.push((item, base_score - idx as i64));
                }
            }
        }

        // Empty query (after checking special providers) - return frecency-sorted items
        if query.is_empty() {
            let items: Vec<(LaunchItem, i64)> = self
                .providers
                .iter()
                .filter(|p| filter.is_active(p.provider_type()))
                .flat_map(|p| p.items().iter().cloned())
                .filter(|item| {
                    // Apply tag filter if present
                    if let Some(tag) = tag_filter {
                        item.tags.iter().any(|t| t.to_lowercase().contains(tag))
                    } else {
                        true
                    }
                })
                .map(|item| {
                    let frecency_score = frecency.get_score(&item.id);
                    let boosted = (frecency_score * frecency_weight * 100.0) as i64;
                    (item, boosted)
                })
                .collect();

            // Combine widgets (already in results) with frecency items
            results.extend(items);
            results.sort_by(|a, b| b.1.cmp(&a.1));
            results.truncate(max_results);
            return results;
        }

        // Regular search with frecency boost and tag matching
        let search_results: Vec<(LaunchItem, i64)> = self
            .providers
            .iter()
            .filter(|provider| filter.is_active(provider.provider_type()))
            .flat_map(|provider| {
                provider.items().iter().filter_map(|item| {
                    // Apply tag filter if present
                    if let Some(tag) = tag_filter
                        && !item.tags.iter().any(|t| t.to_lowercase().contains(tag)) {
                            return None;
                        }

                    let name_score = self.matcher.fuzzy_match(&item.name, query);
                    let desc_score = item
                        .description
                        .as_ref()
                        .and_then(|d| self.matcher.fuzzy_match(d, query));

                    // Also match against tags (lower weight)
                    let tag_score = item
                        .tags
                        .iter()
                        .filter_map(|t| self.matcher.fuzzy_match(t, query))
                        .max()
                        .map(|s| s / 3); // Lower weight for tag matches

                    let base_score = match (name_score, desc_score, tag_score) {
                        (Some(n), Some(d), Some(t)) => Some(n.max(d).max(t)),
                        (Some(n), Some(d), None) => Some(n.max(d)),
                        (Some(n), None, Some(t)) => Some(n.max(t)),
                        (Some(n), None, None) => Some(n),
                        (None, Some(d), Some(t)) => Some((d / 2).max(t)),
                        (None, Some(d), None) => Some(d / 2),
                        (None, None, Some(t)) => Some(t),
                        (None, None, None) => None,
                    };

                    base_score.map(|s| {
                        let frecency_score = frecency.get_score(&item.id);
                        let frecency_boost = (frecency_score * frecency_weight * 10.0) as i64;
                        (item.clone(), s + frecency_boost)
                    })
                })
            })
            .collect();

        results.extend(search_results);
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(max_results);

        #[cfg(feature = "dev-logging")]
        {
            debug!("[Search] Returning {} results", results.len());
            for (i, (item, score)) in results.iter().take(5).enumerate() {
                debug!("[Search]   #{}: {} (score={}, provider={:?})", i + 1, item.name, score, item.provider);
            }
            if results.len() > 5 {
                debug!("[Search]   ... and {} more", results.len() - 5);
            }
        }

        results
    }

    /// Get all available provider types (for UI tabs)
    #[allow(dead_code)]
    pub fn available_providers(&self) -> Vec<ProviderType> {
        self.providers.iter().map(|p| p.provider_type()).collect()
    }

    /// Get a widget item by type_id (e.g., "pomodoro", "weather", "media")
    /// Returns the first item from the widget provider, if any
    pub fn get_widget_item(&self, type_id: &str) -> Option<LaunchItem> {
        self.widget_providers
            .iter()
            .find(|p| p.type_id() == type_id)
            .and_then(|p| p.items().first().cloned())
    }

    /// Get all loaded widget provider type_ids
    /// Returns an iterator over the type_ids of currently loaded widget providers
    pub fn widget_type_ids(&self) -> impl Iterator<Item = &str> {
        self.widget_providers.iter().map(|p| p.type_id())
    }

    /// Query a plugin for submenu actions
    ///
    /// This is used when a user selects a SUBMENU:plugin_id:data item.
    /// The plugin is queried with "?SUBMENU:data" and returns action items.
    ///
    /// Returns (display_name, actions) where display_name is the item name
    /// and actions are the submenu items returned by the plugin.
    pub fn query_submenu_actions(
        &self,
        plugin_id: &str,
        data: &str,
        display_name: &str,
    ) -> Option<(String, Vec<LaunchItem>)> {
        // Build the submenu query
        let submenu_query = format!("?SUBMENU:{}", data);

        #[cfg(feature = "dev-logging")]
        debug!(
            "[Submenu] Querying plugin '{}' with: {}",
            plugin_id, submenu_query
        );

        // Search in dynamic providers
        for provider in &self.dynamic_providers {
            if provider.type_id() == plugin_id {
                let actions = provider.query(&submenu_query);
                if !actions.is_empty() {
                    return Some((display_name.to_string(), actions));
                }
            }
        }

        // Search in widget providers
        for provider in &self.widget_providers {
            if provider.type_id() == plugin_id {
                let actions = provider.query(&submenu_query);
                if !actions.is_empty() {
                    return Some((display_name.to_string(), actions));
                }
            }
        }

        // Search in static providers (boxed)
        // Note: Static providers don't typically have submenu support,
        // but we check for completeness
        for provider in &self.providers {
            if let ProviderType::Plugin(type_id) = provider.provider_type()
                && type_id == plugin_id
            {
                // Static providers use the items() method, not query
                // Submenu support requires dynamic query capability
                #[cfg(feature = "dev-logging")]
                debug!(
                    "[Submenu] Plugin '{}' is static, cannot query for submenu",
                    plugin_id
                );
            }
        }

        #[cfg(feature = "dev-logging")]
        debug!("[Submenu] No submenu actions found for plugin '{}'", plugin_id);

        None
    }
}
