// Core providers (no plugin equivalents)
mod application;
mod command;

// Native plugin bridge
pub mod native_provider;

// Lua plugin bridge (optional)
#[cfg(feature = "lua")]
pub mod lua_provider;

// Re-exports for core providers
pub use application::ApplicationProvider;
pub use command::CommandProvider;

// Re-export native provider for plugin loading
pub use native_provider::NativeProvider;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use log::info;

#[cfg(feature = "dev-logging")]
use log::debug;

use crate::config::Config;
use crate::data::FrecencyStore;
use crate::plugins::runtime_loader::LoadedRuntime;

/// Metadata descriptor for an available provider (used by IPC/daemon API)
#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub icon: String,
    pub position: String,
}

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
pub trait Provider: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn refresh(&mut self);
    fn items(&self) -> &[LaunchItem];
}

/// Manages all providers and handles searching
pub struct ProviderManager {
    /// Core static providers (apps, commands, dmenu)
    providers: Vec<Box<dyn Provider>>,
    /// Static native plugin providers (need query() for submenu support)
    static_native_providers: Vec<NativeProvider>,
    /// Dynamic providers from native plugins (calculator, websearch, filesearch)
    /// These are queried per-keystroke, not cached
    dynamic_providers: Vec<NativeProvider>,
    /// Widget providers from native plugins (weather, media, pomodoro)
    /// These appear at the top of results
    widget_providers: Vec<NativeProvider>,
    /// Fuzzy matcher for search
    matcher: SkimMatcherV2,
    /// Loaded script runtimes (Lua, Rune) — must stay alive to keep Library handles
    runtimes: Vec<LoadedRuntime>,
    /// Type IDs of providers from script runtimes (for hot-reload removal)
    runtime_type_ids: std::collections::HashSet<String>,
}

impl ProviderManager {
    /// Create a new ProviderManager with core providers and native plugins.
    ///
    /// Core providers (e.g., ApplicationProvider, CommandProvider, DmenuProvider) are
    /// passed in by the caller. Native plugins are categorized based on their declared
    /// ProviderKind and ProviderPosition.
    pub fn new(
        core_providers: Vec<Box<dyn Provider>>,
        native_providers: Vec<NativeProvider>,
    ) -> Self {
        let mut manager = Self {
            providers: core_providers,
            static_native_providers: Vec::new(),
            dynamic_providers: Vec::new(),
            widget_providers: Vec::new(),
            matcher: SkimMatcherV2::default(),
            runtimes: Vec::new(),
            runtime_type_ids: std::collections::HashSet::new(),
        };

        // Categorize native plugins based on their declared ProviderKind and ProviderPosition
        for provider in native_providers {
            let type_id = provider.type_id();

            if provider.is_dynamic() {
                info!(
                    "Registered dynamic provider: {} ({})",
                    provider.name(),
                    type_id
                );
                manager.dynamic_providers.push(provider);
            } else if provider.is_widget() {
                info!(
                    "Registered widget provider: {} ({})",
                    provider.name(),
                    type_id
                );
                manager.widget_providers.push(provider);
            } else {
                info!(
                    "Registered static provider: {} ({})",
                    provider.name(),
                    type_id
                );
                manager.static_native_providers.push(provider);
            }
        }

        // Initial refresh
        manager.refresh_all();

        manager
    }

    /// Create a self-contained ProviderManager from config.
    ///
    /// Loads native plugins, creates core providers (Application + Command),
    /// categorizes everything, and performs initial refresh. Used by the daemon
    /// which doesn't have the UI-driven setup path from `app.rs`.
    pub fn new_with_config(config: &Config) -> Self {
        use crate::plugins::native_loader::NativePluginLoader;
        use std::sync::Arc;

        // Create core providers
        let mut core_providers: Vec<Box<dyn Provider>> = vec![
            Box::new(ApplicationProvider::new()),
            Box::new(CommandProvider::new()),
        ];

        // Load native plugins
        let mut loader = NativePluginLoader::new();
        loader.set_disabled(config.plugins.disabled_plugins.clone());

        let native_providers = match loader.discover() {
            Ok(count) => {
                if count == 0 {
                    info!("No native plugins found");
                    Vec::new()
                } else {
                    info!("Discovered {} native plugin(s)", count);
                    let plugins: Vec<Arc<crate::plugins::native_loader::NativePlugin>> =
                        loader.into_plugins();
                    let mut providers = Vec::new();
                    for plugin in plugins {
                        for provider_info in &plugin.providers {
                            let provider =
                                NativeProvider::new(Arc::clone(&plugin), provider_info.clone());
                            info!(
                                "Created native provider: {} ({})",
                                provider.name(),
                                provider.type_id()
                            );
                            providers.push(provider);
                        }
                    }
                    providers
                }
            }
            Err(e) => {
                log::warn!("Failed to discover native plugins: {}", e);
                Vec::new()
            }
        };

        // Load script runtimes (Lua, Rune) for user plugins
        let mut runtime_providers: Vec<Box<dyn Provider>> = Vec::new();
        let mut runtimes: Vec<LoadedRuntime> = Vec::new();
        let mut runtime_type_ids = std::collections::HashSet::new();
        let owlry_version = env!("CARGO_PKG_VERSION");

        let skip_runtimes = std::env::var("OWLRY_SKIP_RUNTIMES").is_ok();
        if !skip_runtimes
            && let Some(plugins_dir) = crate::paths::plugins_dir()
        {
            // Try Lua runtime
            match LoadedRuntime::load_lua(&plugins_dir, owlry_version) {
                Ok(rt) => {
                    info!("Loaded Lua runtime with {} provider(s)", rt.providers().len());
                    for provider in rt.create_providers() {
                        let type_id = format!("{}", provider.provider_type());
                        runtime_type_ids.insert(type_id);
                        runtime_providers.push(provider);
                    }
                    runtimes.push(rt);
                }
                Err(e) => {
                    info!("Lua runtime not available: {}", e);
                }
            }

            // Try Rune runtime
            match LoadedRuntime::load_rune(&plugins_dir, owlry_version) {
                Ok(rt) => {
                    info!("Loaded Rune runtime with {} provider(s)", rt.providers().len());
                    for provider in rt.create_providers() {
                        let type_id = format!("{}", provider.provider_type());
                        runtime_type_ids.insert(type_id);
                        runtime_providers.push(provider);
                    }
                    runtimes.push(rt);
                }
                Err(e) => {
                    info!("Rune runtime not available: {}", e);
                }
            }
        } // skip_runtimes

        // Merge runtime providers into core providers
        for provider in runtime_providers {
            info!("Registered runtime provider: {}", provider.name());
            core_providers.push(provider);
        }

        let mut manager = Self::new(core_providers, native_providers);
        manager.runtimes = runtimes;
        manager.runtime_type_ids = runtime_type_ids;
        manager
    }

    /// Reload all script runtime providers (called by filesystem watcher)
    pub fn reload_runtimes(&mut self) {
        use crate::plugins::runtime_loader::LoadedRuntime;

        // Remove old runtime providers from the core providers list
        self.providers.retain(|p| {
            let type_str = format!("{}", p.provider_type());
            !self.runtime_type_ids.contains(&type_str)
        });

        // Drop old runtimes (catch panics from runtime cleanup)
        let old_runtimes = std::mem::take(&mut self.runtimes);
        drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drop(old_runtimes);
        })));
        self.runtime_type_ids.clear();

        let owlry_version = env!("CARGO_PKG_VERSION");
        let plugins_dir = match crate::paths::plugins_dir() {
            Some(d) => d,
            None => return,
        };

        // Reload Lua runtime
        match LoadedRuntime::load_lua(&plugins_dir, owlry_version) {
            Ok(rt) => {
                info!("Reloaded Lua runtime with {} provider(s)", rt.providers().len());
                for provider in rt.create_providers() {
                    let type_id = format!("{}", provider.provider_type());
                    self.runtime_type_ids.insert(type_id);
                    self.providers.push(provider);
                }
                self.runtimes.push(rt);
            }
            Err(e) => {
                info!("Lua runtime not available on reload: {}", e);
            }
        }

        // Reload Rune runtime
        match LoadedRuntime::load_rune(&plugins_dir, owlry_version) {
            Ok(rt) => {
                info!("Reloaded Rune runtime with {} provider(s)", rt.providers().len());
                for provider in rt.create_providers() {
                    let type_id = format!("{}", provider.provider_type());
                    self.runtime_type_ids.insert(type_id);
                    self.providers.push(provider);
                }
                self.runtimes.push(rt);
            }
            Err(e) => {
                info!("Rune runtime not available on reload: {}", e);
            }
        }

        // Refresh the newly added providers
        for provider in &mut self.providers {
            provider.refresh();
        }

        info!("Runtime reload complete");
    }

    #[allow(dead_code)]
    pub fn is_dmenu_mode(&self) -> bool {
        self.providers
            .iter()
            .any(|p| p.provider_type() == ProviderType::Dmenu)
    }

    pub fn refresh_all(&mut self) {
        // Refresh core providers (apps, commands)
        for provider in &mut self.providers {
            provider.refresh();
            info!(
                "Provider '{}' loaded {} items",
                provider.name(),
                provider.items().len()
            );
        }

        // Refresh static native providers (clipboard, emoji, ssh, etc.)
        for provider in &mut self.static_native_providers {
            provider.refresh();
            info!(
                "Static provider '{}' loaded {} items",
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
    /// Searches in all native provider lists (static, dynamic, widget)
    pub fn find_native_provider(&self, type_id: &str) -> Option<&NativeProvider> {
        // Check static native providers first (clipboard, emoji, ssh, systemd, etc.)
        if let Some(p) = self
            .static_native_providers
            .iter()
            .find(|p| p.type_id() == type_id)
        {
            return Some(p);
        }
        // Check widget providers (pomodoro, weather, media)
        if let Some(p) = self
            .widget_providers
            .iter()
            .find(|p| p.type_id() == type_id)
        {
            return Some(p);
        }
        // Then dynamic providers (calc, websearch, filesearch)
        self.dynamic_providers
            .iter()
            .find(|p| p.type_id() == type_id)
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

    /// Iterate over all static provider items (core + native static plugins)
    fn all_static_items(&self) -> impl Iterator<Item = &LaunchItem> {
        self.providers.iter().flat_map(|p| p.items().iter()).chain(
            self.static_native_providers
                .iter()
                .flat_map(|p| p.items().iter()),
        )
    }

    #[allow(dead_code)]
    pub fn search(&self, query: &str, max_results: usize) -> Vec<(LaunchItem, i64)> {
        if query.is_empty() {
            // Return recent/popular items when query is empty
            return self
                .all_static_items()
                .take(max_results)
                .map(|item| (item.clone(), 0))
                .collect();
        }

        let mut results: Vec<(LaunchItem, i64)> = self
            .all_static_items()
            .filter_map(|item| {
                // Match against name and description
                let name_score = self.matcher.fuzzy_match(&item.name, query);
                let desc_score = item
                    .description
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
        // Collect items from core providers
        let core_items = self
            .providers
            .iter()
            .filter(|p| filter.is_active(p.provider_type()))
            .flat_map(|p| p.items().iter().cloned());

        // Collect items from static native providers
        let native_items = self
            .static_native_providers
            .iter()
            .filter(|p| filter.is_active(p.provider_type()))
            .flat_map(|p| p.items().iter().cloned());

        if query.is_empty() {
            return core_items
                .chain(native_items)
                .take(max_results)
                .map(|item| (item, 0))
                .collect();
        }

        let mut results: Vec<(LaunchItem, i64)> = core_items
            .chain(native_items)
            .filter_map(|item| {
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

                score.map(|s| (item, s))
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
        debug!(
            "[Search] query={:?}, max={}, frecency_weight={}",
            query, max_results, frecency_weight
        );

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
            // Collect items from core providers
            let core_items = self
                .providers
                .iter()
                .filter(|p| filter.is_active(p.provider_type()))
                .flat_map(|p| p.items().iter().cloned());

            // Collect items from static native providers
            let native_items = self
                .static_native_providers
                .iter()
                .filter(|p| filter.is_active(p.provider_type()))
                .flat_map(|p| p.items().iter().cloned());

            let items: Vec<(LaunchItem, i64)> = core_items
                .chain(native_items)
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
        // Helper closure for scoring items
        let score_item = |item: &LaunchItem| -> Option<(LaunchItem, i64)> {
            // Apply tag filter if present
            if let Some(tag) = tag_filter
                && !item.tags.iter().any(|t| t.to_lowercase().contains(tag))
            {
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
        };

        // Search core providers
        for provider in &self.providers {
            if !filter.is_active(provider.provider_type()) {
                continue;
            }
            for item in provider.items() {
                if let Some(scored) = score_item(item) {
                    results.push(scored);
                }
            }
        }

        // Search static native providers
        for provider in &self.static_native_providers {
            if !filter.is_active(provider.provider_type()) {
                continue;
            }
            for item in provider.items() {
                if let Some(scored) = score_item(item) {
                    results.push(scored);
                }
            }
        }
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(max_results);

        #[cfg(feature = "dev-logging")]
        {
            debug!("[Search] Returning {} results", results.len());
            for (i, (item, score)) in results.iter().take(5).enumerate() {
                debug!(
                    "[Search]   #{}: {} (score={}, provider={:?})",
                    i + 1,
                    item.name,
                    score,
                    item.provider
                );
            }
            if results.len() > 5 {
                debug!("[Search]   ... and {} more", results.len() - 5);
            }
        }

        results
    }

    /// Get all available provider types (for UI tabs)
    #[allow(dead_code)]
    pub fn available_provider_types(&self) -> Vec<ProviderType> {
        self.providers
            .iter()
            .map(|p| p.provider_type())
            .chain(
                self.static_native_providers
                    .iter()
                    .map(|p| p.provider_type()),
            )
            .collect()
    }

    /// Get descriptors for all registered providers (core + native plugins).
    ///
    /// Used by the IPC server to report what providers are available to clients.
    pub fn available_providers(&self) -> Vec<ProviderDescriptor> {
        let mut descs = Vec::new();

        // Core providers
        for provider in &self.providers {
            let (id, prefix, icon) = match provider.provider_type() {
                ProviderType::Application => (
                    "app".to_string(),
                    Some(":app".to_string()),
                    "application-x-executable".to_string(),
                ),
                ProviderType::Command => (
                    "cmd".to_string(),
                    Some(":cmd".to_string()),
                    "utilities-terminal".to_string(),
                ),
                ProviderType::Dmenu => {
                    ("dmenu".to_string(), None, "view-list-symbolic".to_string())
                }
                ProviderType::Plugin(type_id) => (type_id, None, "application-x-addon".to_string()),
            };
            descs.push(ProviderDescriptor {
                id,
                name: provider.name().to_string(),
                prefix,
                icon,
                position: "normal".to_string(),
            });
        }

        // Static native plugin providers
        for provider in &self.static_native_providers {
            descs.push(ProviderDescriptor {
                id: provider.type_id().to_string(),
                name: provider.name().to_string(),
                prefix: provider.prefix().map(String::from),
                icon: provider.icon().to_string(),
                position: provider.position_str().to_string(),
            });
        }

        // Dynamic native plugin providers
        for provider in &self.dynamic_providers {
            descs.push(ProviderDescriptor {
                id: provider.type_id().to_string(),
                name: provider.name().to_string(),
                prefix: provider.prefix().map(String::from),
                icon: provider.icon().to_string(),
                position: provider.position_str().to_string(),
            });
        }

        // Widget native plugin providers
        for provider in &self.widget_providers {
            descs.push(ProviderDescriptor {
                id: provider.type_id().to_string(),
                name: provider.name().to_string(),
                prefix: provider.prefix().map(String::from),
                icon: provider.icon().to_string(),
                position: provider.position_str().to_string(),
            });
        }

        descs
    }

    /// Refresh a specific provider by its type_id.
    ///
    /// Searches core providers (by ProviderType string), static native providers,
    /// and widget providers. Dynamic providers are skipped (they query on demand).
    pub fn refresh_provider(&mut self, provider_id: &str) {
        // Check core providers
        for provider in &mut self.providers {
            let matches = match provider.provider_type() {
                ProviderType::Application => provider_id == "app",
                ProviderType::Command => provider_id == "cmd",
                ProviderType::Dmenu => provider_id == "dmenu",
                ProviderType::Plugin(ref id) => provider_id == id,
            };
            if matches {
                provider.refresh();
                info!("Refreshed core provider '{}'", provider.name());
                return;
            }
        }

        // Check static native providers
        for provider in &mut self.static_native_providers {
            if provider.type_id() == provider_id {
                provider.refresh();
                info!("Refreshed static provider '{}'", provider.name());
                return;
            }
        }

        // Check widget providers
        for provider in &mut self.widget_providers {
            if provider.type_id() == provider_id {
                provider.refresh();
                info!("Refreshed widget provider '{}'", provider.name());
                return;
            }
        }

        info!("Provider '{}' not found for refresh", provider_id);
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

        // Search in static native providers (clipboard, emoji, ssh, systemd, etc.)
        for provider in &self.static_native_providers {
            if provider.type_id() == plugin_id {
                let actions = provider.query(&submenu_query);
                if !actions.is_empty() {
                    return Some((display_name.to_string(), actions));
                }
            }
        }

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

        #[cfg(feature = "dev-logging")]
        debug!(
            "[Submenu] No submenu actions found for plugin '{}'",
            plugin_id
        );

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal mock provider for testing ProviderManager
    struct MockProvider {
        name: String,
        provider_type: ProviderType,
        items: Vec<LaunchItem>,
        refresh_count: usize,
    }

    impl MockProvider {
        fn new(name: &str, provider_type: ProviderType) -> Self {
            Self {
                name: name.to_string(),
                provider_type,
                items: Vec::new(),
                refresh_count: 0,
            }
        }

        fn with_items(mut self, items: Vec<LaunchItem>) -> Self {
            self.items = items;
            self
        }
    }

    impl Provider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn provider_type(&self) -> ProviderType {
            self.provider_type.clone()
        }

        fn refresh(&mut self) {
            self.refresh_count += 1;
        }

        fn items(&self) -> &[LaunchItem] {
            &self.items
        }
    }

    fn make_item(id: &str, name: &str, provider: ProviderType) -> LaunchItem {
        LaunchItem {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            icon: None,
            provider,
            command: format!("run-{}", id),
            terminal: false,
            tags: Vec::new(),
        }
    }

    #[test]
    fn test_available_providers_core_only() {
        let providers: Vec<Box<dyn Provider>> = vec![
            Box::new(MockProvider::new("Applications", ProviderType::Application)),
            Box::new(MockProvider::new("Commands", ProviderType::Command)),
        ];
        let pm = ProviderManager::new(providers, Vec::new());
        let descs = pm.available_providers();
        assert_eq!(descs.len(), 2);
        assert_eq!(descs[0].id, "app");
        assert_eq!(descs[0].name, "Applications");
        assert_eq!(descs[0].prefix, Some(":app".to_string()));
        assert_eq!(descs[0].icon, "application-x-executable");
        assert_eq!(descs[0].position, "normal");
        assert_eq!(descs[1].id, "cmd");
        assert_eq!(descs[1].name, "Commands");
    }

    #[test]
    fn test_available_providers_dmenu() {
        let providers: Vec<Box<dyn Provider>> =
            vec![Box::new(MockProvider::new("dmenu", ProviderType::Dmenu))];
        let pm = ProviderManager::new(providers, Vec::new());
        let descs = pm.available_providers();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].id, "dmenu");
        assert!(descs[0].prefix.is_none());
    }

    #[test]
    fn test_available_provider_types() {
        let providers: Vec<Box<dyn Provider>> = vec![
            Box::new(MockProvider::new("Applications", ProviderType::Application)),
            Box::new(MockProvider::new("Commands", ProviderType::Command)),
        ];
        let pm = ProviderManager::new(providers, Vec::new());
        let types = pm.available_provider_types();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&ProviderType::Application));
        assert!(types.contains(&ProviderType::Command));
    }

    #[test]
    fn test_refresh_provider_core() {
        let app = MockProvider::new("Applications", ProviderType::Application);
        let cmd = MockProvider::new("Commands", ProviderType::Command);
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(app), Box::new(cmd)];
        let mut pm = ProviderManager::new(providers, Vec::new());

        // refresh_all was called during construction, now refresh individual
        pm.refresh_provider("app");
        pm.refresh_provider("cmd");
        // Just verifying it doesn't panic; can't easily inspect refresh_count
        // through Box<dyn Provider>
    }

    #[test]
    fn test_refresh_provider_unknown_does_not_panic() {
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider::new(
            "Applications",
            ProviderType::Application,
        ))];
        let mut pm = ProviderManager::new(providers, Vec::new());
        pm.refresh_provider("nonexistent");
        // Should complete without panicking
    }

    #[test]
    fn test_search_with_core_providers() {
        let items = vec![
            make_item("firefox", "Firefox", ProviderType::Application),
            make_item("vim", "Vim", ProviderType::Application),
        ];
        let provider =
            MockProvider::new("Applications", ProviderType::Application).with_items(items);
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(provider)];
        let pm = ProviderManager::new(providers, Vec::new());

        let results = pm.search("fire", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.name, "Firefox");
    }
}
