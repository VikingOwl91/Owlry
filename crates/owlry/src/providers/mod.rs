// Core providers (compiled in)
mod application;
pub(crate) mod calculator;
mod command;
pub(crate) mod converter;
pub mod dmenu;
pub(crate) mod power;

// Optional feature-gated providers
#[cfg(feature = "clipboard")]
pub(crate) mod clipboard;
#[cfg(feature = "emoji")]
pub(crate) mod emoji;
#[cfg(feature = "filesearch")]
pub(crate) mod filesearch;
#[cfg(feature = "rofimoji")]
pub(crate) mod rofimoji;
#[cfg(feature = "ssh")]
pub(crate) mod ssh;
#[cfg(feature = "systemd")]
pub(crate) mod systemd;
#[cfg(feature = "websearch")]
pub(crate) mod websearch;

// Re-exports for core providers
pub use application::ApplicationProvider;
pub use command::CommandProvider;
pub use dmenu::DmenuProvider;

use chrono::{DateTime, Utc};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use log::info;
use serde::{Deserialize, Serialize};

#[cfg(feature = "dev-logging")]
use log::debug;

use std::sync::{Arc, RwLock};

use crate::config::Config;
use crate::data::FrecencyStore;

/// Where a provider sits in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPosition {
    /// Normal results list.
    Normal,
    /// Widget — rendered at the top of the results, outside the normal flow.
    Widget,
}

impl ProviderPosition {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderPosition::Normal => "normal",
            ProviderPosition::Widget => "widget",
        }
    }
}

/// Whether a provider exposes a cached item list (static) or generates
/// results on every keystroke (dynamic). Surfaced by `owlry doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Static,
    Dynamic,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Static => "static",
            ProviderKind::Dynamic => "dynamic",
        }
    }
}

/// Metadata descriptor for an available provider (used by IPC/daemon API)
#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub icon: String,
    pub position: String,
    pub tab_label: Option<String>,
    pub search_noun: Option<String>,
    pub kind: ProviderKind,
}

/// Trust level of a [`LaunchItem`]'s command, used to gate `sh -c` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemSource {
    /// Built-in provider compiled into the binary (trusted).
    Core,
    /// Script-defined provider (Lua, in Phase 3+) — user-installed, untrusted.
    ScriptPlugin,
}

impl ItemSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemSource::Core => "core",
            ItemSource::ScriptPlugin => "script_plugin",
        }
    }
}

impl std::str::FromStr for ItemSource {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "script_plugin" => Ok(ItemSource::ScriptPlugin),
            _ => Ok(ItemSource::Core),
        }
    }
}

/// Typed instruction for what selecting a result does.
///
/// `command` remains the launch payload for compatibility with older IPC
/// peers; control flow such as submenus and provider-internal actions lives
/// here instead of being inferred from string prefixes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemActivation {
    #[default]
    Launch,
    Submenu {
        provider_id: String,
        data: String,
    },
    PluginAction {
        command: String,
    },
}

impl ItemActivation {
    pub fn is_launch(&self) -> bool {
        matches!(self, Self::Launch)
    }

    /// Transitional decoder for results produced by pre-typed daemons and
    /// script providers still using the historical command prefix.
    pub fn from_legacy_command(command: &str) -> Self {
        if command.starts_with('!') {
            return Self::PluginAction {
                command: command.to_string(),
            };
        }
        let Some(rest) = command.strip_prefix("SUBMENU:") else {
            return Self::Launch;
        };
        let Some((provider_id, data)) = rest.split_once(':') else {
            return Self::Launch;
        };
        if provider_id.is_empty() || data.is_empty() {
            return Self::Launch;
        }
        Self::Submenu {
            provider_id: provider_id.to_string(),
            data: data.to_string(),
        }
    }
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
    pub activation: ItemActivation,
    /// Tags/categories for filtering (e.g., from .desktop Categories)
    pub tags: Vec<String>,
    /// Trust level — gates `sh -c` execution for script plugin items.
    pub source: ItemSource,
}

/// Provider type identifier for filtering and badge display.
///
/// - `Application`, `Command`, `Dmenu`: built-in core providers
/// - `Plugin(type_id)`: any other provider (compiled-in feature module or
///   future Lua-registered provider). The `type_id` is the provider's
///   stable identifier (e.g. `"bookmarks"`, `"uuctl"`, `"calc"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderType {
    Application,
    Command,
    Dmenu,
    Plugin(String),
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "app" | "apps" | "application" | "applications" => Ok(ProviderType::Application),
            "cmd" | "cmds" | "command" | "commands" => Ok(ProviderType::Command),
            "dmenu" => Ok(ProviderType::Dmenu),
            // Power provider — `sys` and `system` are pre-v2 muscle-memory aliases.
            "power" | "sys" | "system" => Ok(ProviderType::Plugin("power".into())),
            // systemd provider — type_id is `uuctl` (CLI back-compat).
            "uuctl" | "systemd" => Ok(ProviderType::Plugin("uuctl".into())),
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

/// Trait for all search providers.
///
/// Static providers hold a refreshable item cache. Submenu and action support
/// are optional methods — implement them only if the provider produces typed
/// submenu activations or handles plugin-defined action commands.
pub trait Provider: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn refresh(&mut self);
    fn items(&self) -> &[LaunchItem];

    /// Short label for UI tab button (e.g., "Shutdown"). None = use default.
    fn tab_label(&self) -> Option<&str> {
        None
    }
    /// Noun for search placeholder (e.g., "shutdown actions"). None = use default.
    fn search_noun(&self) -> Option<&str> {
        None
    }
    /// Optional search prefix (e.g. ":bm"). None = no prefix.
    fn prefix(&self) -> Option<&str> {
        None
    }
    /// Icon name (XDG icon theme).
    fn icon(&self) -> &str {
        "application-x-addon"
    }
    /// UI placement.
    fn position(&self) -> ProviderPosition {
        ProviderPosition::Normal
    }
    /// Priority hint for ordering.
    fn priority(&self) -> u32 {
        0
    }

    /// Generate submenu actions for an item's [`ItemActivation::Submenu`].
    /// `data` is the provider-owned payload from that activation.
    /// Default: no submenu support.
    fn submenu_actions(&self, _data: &str) -> Vec<LaunchItem> {
        Vec::new()
    }

    /// Handle a plugin-defined action command (e.g. `"POMODORO:start"`).
    /// Returns true if the command was handled.
    /// Default: no action support.
    fn execute_action(&self, _command: &str) -> bool {
        false
    }
}

/// Trait for built-in providers that produce results per-keystroke.
/// Unlike static `Provider`s which cache items via `refresh()`/`items()`,
/// dynamic providers generate results on every query.
pub trait DynamicProvider: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn query(&self, query: &str) -> Vec<LaunchItem>;
    fn priority(&self) -> u32;

    /// Whether this provider contributes to bare auto-mode queries (no prefix).
    ///
    /// Cheap in-process providers (calculator, converter, web search) return
    /// `true` and run on every keystroke. Providers that shell out and do I/O
    /// per query (file search) return `false`, so they only run when their
    /// prefix is explicitly active — otherwise every keystroke spawns a
    /// subprocess and stalls the whole result set.
    fn runs_in_auto_mode(&self) -> bool {
        true
    }

    /// Optional search prefix (e.g. ":calc"). None = no prefix.
    fn prefix(&self) -> Option<&str> {
        None
    }
    /// Icon name (XDG icon theme).
    fn icon(&self) -> &str {
        "application-x-addon"
    }
    /// Tab button label.
    fn tab_label(&self) -> Option<&str> {
        None
    }
    /// Search placeholder noun.
    fn search_noun(&self) -> Option<&str> {
        None
    }

    /// Handle a plugin action command. Returns true if handled.
    fn execute_action(&self, _command: &str) -> bool {
        false
    }
}

/// Manages all providers and handles searching.
pub struct ProviderManager {
    /// Static providers (apps, commands, systemd, etc.).
    providers: Vec<Box<dyn Provider>>,
    /// Dynamic providers (calculator, converter, websearch, filesearch).
    /// Queried per-keystroke, not cached.
    builtin_dynamic: Vec<Box<dyn DynamicProvider>>,
    /// Fuzzy matcher for search.
    matcher: SkimMatcherV2,
}

impl ProviderManager {
    /// Create a new ProviderManager from a pre-built list of providers.
    ///
    /// Used by tests and by the dmenu-mode client. The daemon uses
    /// [`Self::new_with_config`] which builds the provider set from config.
    pub fn new(
        core_providers: Vec<Box<dyn Provider>>,
        dynamic: Vec<Box<dyn DynamicProvider>>,
    ) -> Self {
        let mut manager = Self {
            providers: core_providers,
            builtin_dynamic: dynamic,
            matcher: SkimMatcherV2::default(),
        };
        manager.refresh_all();
        manager
    }

    /// Build a ProviderManager for the daemon, sourcing enabled providers from config.
    ///
    /// `user_providers` are appended after the built-ins — typically these
    /// are `LuaProvider`s constructed from `owlry.provider {}` registrations
    /// (Phase 3.3+). Pass an empty Vec if there are no user providers, or
    /// when the `lua` feature is off.
    pub fn new_with_config(
        config: Arc<RwLock<Config>>,
        user_providers: Vec<Box<dyn Provider>>,
    ) -> Self {
        let cfg_snapshot = match config.read() {
            Ok(cfg) => cfg.providers.clone(),
            Err(_) => {
                log::warn!("Config lock poisoned during provider init; using defaults");
                crate::config::ProvidersConfig::default()
            }
        };

        Self::new_with_provider_config(cfg_snapshot, user_providers)
    }

    /// Build a manager from an already-resolved provider configuration.
    ///
    /// Reload paths use this variant to prepare the complete replacement
    /// before taking the daemon's state locks, so config, providers, and the
    /// Lua context can be swapped together without exposing a half-built
    /// manager.
    pub(crate) fn new_with_provider_config(
        cfg_snapshot: crate::config::ProvidersConfig,
        user_providers: Vec<Box<dyn Provider>>,
    ) -> Self {
        let mut core_providers: Vec<Box<dyn Provider>> = vec![
            Box::new(ApplicationProvider::new()),
            Box::new(CommandProvider::new()),
        ];

        if cfg_snapshot.power {
            core_providers.push(Box::new(power::PowerProvider::new()));
            info!("Registered built-in power provider");
        }

        #[cfg(feature = "clipboard")]
        if cfg_snapshot.clipboard {
            core_providers.push(Box::new(clipboard::ClipboardProvider::new()));
            info!("Registered clipboard provider");
        }
        #[cfg(feature = "emoji")]
        if cfg_snapshot.emoji {
            core_providers.push(Box::new(emoji::EmojiProvider::new()));
            info!("Registered emoji provider");
        }
        #[cfg(feature = "rofimoji")]
        if cfg_snapshot.rofimoji {
            core_providers.push(Box::new(rofimoji::RofimojiProvider::new()));
            info!("Registered rofimoji provider");
        }
        #[cfg(feature = "ssh")]
        if cfg_snapshot.ssh {
            core_providers.push(Box::new(ssh::SshProvider::new()));
            info!("Registered ssh provider");
        }
        #[cfg(feature = "systemd")]
        if cfg_snapshot.systemd {
            core_providers.push(Box::new(systemd::SystemdProvider::new()));
            info!("Registered systemd provider (type_id: uuctl)");
        }

        // Lua-defined user providers join the static set after built-ins.
        // They were enumerated in the LuaConfig snapshot during Config load
        // and turned into Box<dyn Provider> by the caller (see Server::bind).
        if !user_providers.is_empty() {
            info!(
                "Registering {} user-defined provider(s)",
                user_providers.len()
            );
            core_providers.extend(user_providers);
        }

        let mut builtin_dynamic: Vec<Box<dyn DynamicProvider>> = Vec::new();
        if cfg_snapshot.calculator {
            builtin_dynamic.push(Box::new(calculator::CalculatorProvider));
            info!("Registered built-in calculator provider");
        }
        if cfg_snapshot.converter {
            builtin_dynamic.push(Box::new(converter::ConverterProvider::new()));
            info!("Registered built-in converter provider");
        }
        #[cfg(feature = "filesearch")]
        if cfg_snapshot.filesearch {
            builtin_dynamic.push(Box::new(filesearch::FileSearchProvider::with_roots(
                cfg_snapshot.filesearch_roots.clone(),
            )));
            info!("Registered filesearch provider");
        }
        #[cfg(feature = "websearch")]
        if cfg_snapshot.websearch {
            builtin_dynamic.push(Box::new(websearch::WebSearchProvider::new()));
            info!("Registered websearch provider");
        }

        Self::new(core_providers, builtin_dynamic)
    }

    #[allow(dead_code)]
    pub fn is_dmenu_mode(&self) -> bool {
        self.providers
            .iter()
            .any(|p| p.provider_type() == ProviderType::Dmenu)
    }

    pub fn refresh_all(&mut self) {
        for provider in &mut self.providers {
            provider.refresh();
            info!(
                "Provider '{}' loaded {} items",
                provider.name(),
                provider.items().len()
            );
        }
        // Dynamic providers don't need refresh (they query on demand).
    }

    /// Register an additional provider at runtime.
    ///
    /// Used by Phase 3+ Lua config to add user-defined providers after the
    /// daemon has booted. The provider's `refresh()` is called immediately.
    #[allow(dead_code)]
    pub fn add_provider(&mut self, mut provider: Box<dyn Provider>) {
        provider.refresh();
        info!("Registered provider: {}", provider.name());
        self.providers.push(provider);
    }

    /// Execute a plugin-defined action command.
    ///
    /// Command format: `PLUGIN_ID:action_data` (e.g. `"POMODORO:start"`).
    /// Returns true if a provider handled the command.
    pub fn execute_plugin_action(&self, command: &str) -> bool {
        for provider in &self.providers {
            if provider.execute_action(command) {
                return true;
            }
        }
        for provider in &self.builtin_dynamic {
            if provider.execute_action(command) {
                return true;
            }
        }
        false
    }

    #[allow(dead_code)]
    pub fn search(&self, query: &str, max_results: usize) -> Vec<(LaunchItem, i64)> {
        if query.is_empty() {
            return self
                .providers
                .iter()
                .flat_map(|p| p.items().iter().cloned())
                .take(max_results)
                .map(|item| (item, 0))
                .collect();
        }

        let mut results: Vec<(LaunchItem, i64)> = self
            .providers
            .iter()
            .flat_map(|p| p.items().iter())
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
                score.map(|s| (item.clone(), s))
            })
            .collect();

        results.sort_by_key(|x| std::cmp::Reverse(x.1));
        results.truncate(max_results);
        results
    }

    /// Search with provider filtering.
    #[allow(dead_code)]
    pub fn search_filtered(
        &self,
        query: &str,
        max_results: usize,
        filter: &crate::filter::ProviderFilter,
        tag_filter: Option<&str>,
    ) -> Vec<(LaunchItem, i64)> {
        let all_items = self
            .providers
            .iter()
            .filter(|p| filter.is_active(p.provider_type()))
            .flat_map(|p| p.items().iter().cloned())
            .filter(|item| tag_filter.is_none_or(|t| item.tags.iter().any(|it| it == t)));

        if query.is_empty() {
            return all_items.take(max_results).map(|item| (item, 0)).collect();
        }

        let mut results: Vec<(LaunchItem, i64)> = all_items
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

        results.sort_by_key(|x| std::cmp::Reverse(x.1));
        results.truncate(max_results);
        results
    }

    /// Search with frecency boosting, dynamic providers, and tag filtering.
    /// Items launched most recently, newest first — backs the `:recent` prefix.
    ///
    /// Resolves frecency ids against live static-provider items (frecency only
    /// stores ids, not launchable items). Dynamic providers are excluded (no
    /// stable cached items), and ids with no surviving item — e.g. an
    /// uninstalled app — are silently dropped.
    pub fn recent_items(
        &self,
        frecency: &FrecencyStore,
        max_results: usize,
    ) -> Vec<(LaunchItem, i64)> {
        let entries = frecency.entries();
        let resolved = self
            .providers
            .iter()
            .filter(|p| p.position() != ProviderPosition::Widget)
            .flat_map(|p| p.items().iter())
            .filter_map(|item| entries.get(&item.id).map(|e| (item, e.last_launch)));
        Self::rank_recent(resolved, max_results)
    }

    /// Pure ranking core for [`recent_items`]: sort resolved `(item, last_launch)`
    /// pairs newest-first, truncate to `max_results`, and assign descending
    /// scores so the order survives the daemon's score-based sort.
    fn rank_recent<'a>(
        resolved: impl Iterator<Item = (&'a LaunchItem, DateTime<Utc>)>,
        max_results: usize,
    ) -> Vec<(LaunchItem, i64)> {
        let mut scored: Vec<(&LaunchItem, DateTime<Utc>)> = resolved.collect();
        scored.sort_by_key(|(_, ts)| std::cmp::Reverse(*ts));
        scored.truncate(max_results);
        scored
            .into_iter()
            .enumerate()
            .map(|(idx, (item, _))| (item.clone(), max_results.saturating_sub(idx) as i64))
            .collect()
    }

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

        let now = Utc::now();
        let mut results: Vec<(LaunchItem, i64)> = Vec::new();

        // Widget providers contribute on empty query (no prefix active).
        if filter.active_prefix().is_none() && query.is_empty() {
            for provider in &self.providers {
                if provider.position() != ProviderPosition::Widget {
                    continue;
                }
                let base_score = provider.priority() as i64;
                for (idx, item) in provider.items().iter().enumerate() {
                    results.push((item.clone(), base_score - idx as i64));
                }
            }
        }

        // Dynamic providers (calculator, converter, etc.) — only when there's a query.
        if !query.is_empty() {
            for provider in &self.builtin_dynamic {
                if !filter.is_active(provider.provider_type()) {
                    continue;
                }
                // Expensive providers (file search) only run when their prefix
                // is explicitly selected — never on a bare auto-mode keystroke.
                let prefix_active = filter
                    .active_prefix()
                    .is_some_and(|p| p == provider.provider_type());
                if !prefix_active && !provider.runs_in_auto_mode() {
                    continue;
                }
                let dynamic_results = provider.query(query);
                let base_score = provider.priority() as i64;
                let grouping_bonus: i64 = match provider.provider_type() {
                    ProviderType::Plugin(ref id) if matches!(id.as_str(), "calc" | "conv") => {
                        10_000
                    }
                    _ => 0,
                };
                for (idx, item) in dynamic_results.into_iter().enumerate() {
                    results.push((item, base_score + grouping_bonus - idx as i64));
                }
            }
        }

        // Empty query (after widgets) — frecency-sorted items.
        if query.is_empty() {
            let mut scored_refs: Vec<(&LaunchItem, i64)> = self
                .providers
                .iter()
                .filter(|p| p.position() != ProviderPosition::Widget)
                .filter(|p| filter.is_active(p.provider_type()))
                .flat_map(|p| p.items().iter())
                .filter(|item| {
                    if let Some(tag) = tag_filter {
                        item.tags.iter().any(|t| t.to_lowercase().contains(tag))
                    } else {
                        true
                    }
                })
                .map(|item| {
                    let frecency_score = frecency.get_score_at(&item.id, now);
                    let boosted = (frecency_score * frecency_weight * 100.0) as i64;
                    (item, boosted)
                })
                .collect();

            if scored_refs.len() > max_results {
                scored_refs.select_nth_unstable_by(max_results, |a, b| b.1.cmp(&a.1));
                scored_refs.truncate(max_results);
            }
            scored_refs.sort_by_key(|x| std::cmp::Reverse(x.1));

            results.extend(
                scored_refs
                    .into_iter()
                    .map(|(item, score)| (item.clone(), score)),
            );
            results.sort_by_key(|x| std::cmp::Reverse(x.1));
            results.truncate(max_results);
            return results;
        }

        // Regular search with frecency boost and tag matching.
        let score_item = |item: &LaunchItem| -> Option<i64> {
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
            let tag_score = item
                .tags
                .iter()
                .filter_map(|t| self.matcher.fuzzy_match(t, query))
                .max()
                .map(|s| s / 3);

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
                let frecency_score = frecency.get_score_at(&item.id, now);
                let frecency_boost = (frecency_score * frecency_weight * 10.0) as i64;
                let exact_match_boost = if item.name.eq_ignore_ascii_case(query) {
                    match &item.provider {
                        ProviderType::Application => 50_000,
                        _ => 30_000,
                    }
                } else {
                    0
                };
                s + frecency_boost + exact_match_boost
            })
        };

        let mut scored_refs: Vec<(&LaunchItem, i64)> = Vec::new();
        for provider in &self.providers {
            if provider.position() == ProviderPosition::Widget {
                continue;
            }
            if !filter.is_active(provider.provider_type()) {
                continue;
            }
            for item in provider.items() {
                if let Some(score) = score_item(item) {
                    scored_refs.push((item, score));
                }
            }
        }

        if scored_refs.len() > max_results {
            scored_refs.select_nth_unstable_by(max_results, |a, b| b.1.cmp(&a.1));
            scored_refs.truncate(max_results);
        }
        scored_refs.sort_by_key(|x| std::cmp::Reverse(x.1));

        results.extend(
            scored_refs
                .into_iter()
                .map(|(item, score)| (item.clone(), score)),
        );
        results.sort_by_key(|x| std::cmp::Reverse(x.1));
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

    /// Get all available provider types (for UI tabs).
    ///
    /// Includes both static `Provider`s and dynamic `DynamicProvider`s so
    /// callers like `owlry doctor` / `owlry providers` see the full picture.
    #[allow(dead_code)]
    pub fn available_provider_types(&self) -> Vec<ProviderType> {
        self.providers
            .iter()
            .map(|p| p.provider_type())
            .chain(self.builtin_dynamic.iter().map(|p| p.provider_type()))
            .collect()
    }

    /// Get descriptors for all registered providers (static + dynamic).
    ///
    /// Used by the IPC server to report what providers are available to clients.
    pub fn available_providers(&self) -> Vec<ProviderDescriptor> {
        let mut descs = Vec::new();

        for provider in &self.providers {
            let (id, default_prefix, default_icon) = match provider.provider_type() {
                ProviderType::Application => (
                    "app".to_string(),
                    Some(":app".to_string()),
                    "application-x-executable",
                ),
                ProviderType::Command => (
                    "cmd".to_string(),
                    Some(":cmd".to_string()),
                    "utilities-terminal",
                ),
                ProviderType::Dmenu => ("dmenu".to_string(), None, "view-list-symbolic"),
                ProviderType::Plugin(type_id) => (type_id, None, "application-x-addon"),
            };
            descs.push(ProviderDescriptor {
                id,
                name: provider.name().to_string(),
                prefix: provider.prefix().map(String::from).or(default_prefix),
                icon: {
                    let trait_icon = provider.icon();
                    if trait_icon == "application-x-addon" {
                        default_icon.to_string()
                    } else {
                        trait_icon.to_string()
                    }
                },
                position: provider.position().as_str().to_string(),
                tab_label: provider.tab_label().map(String::from),
                search_noun: provider.search_noun().map(String::from),
                kind: ProviderKind::Static,
            });
        }

        // Dynamic providers (calc, conv, websearch, filesearch). They're always
        // ProviderType::Plugin(<type_id>) — the only other variants are
        // Application/Command/Dmenu which are static-only.
        for provider in &self.builtin_dynamic {
            let id = match provider.provider_type() {
                ProviderType::Plugin(type_id) => type_id,
                // Defensive: keep the surface honest even if a future dynamic
                // provider claims a non-Plugin type.
                other => other.to_string(),
            };
            descs.push(ProviderDescriptor {
                id,
                name: provider.name().to_string(),
                prefix: provider.prefix().map(String::from),
                icon: provider.icon().to_string(),
                position: ProviderPosition::Normal.as_str().to_string(),
                tab_label: provider.tab_label().map(String::from),
                search_noun: provider.search_noun().map(String::from),
                kind: ProviderKind::Dynamic,
            });
        }

        descs
    }

    /// Refresh a specific provider by its type_id.
    pub fn refresh_provider(&mut self, provider_id: &str) {
        for provider in &mut self.providers {
            let matches = match provider.provider_type() {
                ProviderType::Application => provider_id == "app",
                ProviderType::Command => provider_id == "cmd",
                ProviderType::Dmenu => provider_id == "dmenu",
                ProviderType::Plugin(ref id) => provider_id == id,
            };
            if matches {
                provider.refresh();
                info!("Refreshed provider '{}'", provider.name());
                return;
            }
        }
        info!("Provider '{}' not found for refresh", provider_id);
    }

    /// Query a provider for submenu actions.
    ///
    /// Called when a user selects a typed submenu item. The provider's
    /// `submenu_actions(data)` is invoked to produce the action list.
    pub fn query_submenu_actions(
        &self,
        plugin_id: &str,
        data: &str,
        display_name: &str,
    ) -> Option<(String, Vec<LaunchItem>)> {
        #[cfg(feature = "dev-logging")]
        debug!(
            "[Submenu] Querying provider '{}' with data: {}",
            plugin_id, data
        );

        for provider in &self.providers {
            let matches = match provider.provider_type() {
                ProviderType::Plugin(ref id) => id == plugin_id,
                _ => false,
            };
            if matches {
                let actions = provider.submenu_actions(data);
                if !actions.is_empty() {
                    return Some((display_name.to_string(), actions));
                }
            }
        }

        #[cfg(feature = "dev-logging")]
        debug!("[Submenu] No submenu actions for provider '{}'", plugin_id);
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
            activation: ItemActivation::Launch,
            tags: Vec::new(),
            source: ItemSource::Core,
        }
    }

    #[test]
    fn rank_recent_orders_newest_first_and_truncates() {
        let now = Utc::now();
        let a = make_item("a", "A", ProviderType::Application);
        let b = make_item("b", "B", ProviderType::Application);
        let c = make_item("c", "C", ProviderType::Application);
        // Deliberately unsorted input; c newest, then b, then a.
        let resolved = vec![
            (&a, now - chrono::Duration::days(5)),
            (&c, now),
            (&b, now - chrono::Duration::days(1)),
        ];

        let out = ProviderManager::rank_recent(resolved.into_iter(), 2);

        assert_eq!(out.len(), 2, "truncated to max_results");
        assert_eq!(out[0].0.id, "c", "newest first");
        assert_eq!(out[1].0.id, "b");
        assert!(out[0].1 > out[1].1, "descending score preserves order");
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

        pm.refresh_provider("app");
        pm.refresh_provider("cmd");
        // Just verifying it doesn't panic.
    }

    #[test]
    fn test_refresh_provider_unknown_does_not_panic() {
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider::new(
            "Applications",
            ProviderType::Application,
        ))];
        let mut pm = ProviderManager::new(providers, Vec::new());
        pm.refresh_provider("nonexistent");
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

    /// Dynamic provider that counts how often `query()` is invoked, used to
    /// verify auto-mode gating without shelling out to a real backend.
    struct SpyDynamic {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        auto: bool,
        type_id: String,
    }

    impl DynamicProvider for SpyDynamic {
        fn name(&self) -> &str {
            "Spy"
        }
        fn provider_type(&self) -> ProviderType {
            ProviderType::Plugin(self.type_id.clone())
        }
        fn query(&self, _query: &str) -> Vec<LaunchItem> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Vec::new()
        }
        fn priority(&self) -> u32 {
            100
        }
        fn runs_in_auto_mode(&self) -> bool {
            self.auto
        }
    }

    fn run_spy_search(auto: bool, prefix: Option<ProviderType>) -> usize {
        use crate::filter::ProviderFilter;
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let spy = SpyDynamic {
            calls: calls.clone(),
            auto,
            type_id: "filesearch".into(),
        };
        let pm = ProviderManager::new(Vec::new(), vec![Box::new(spy)]);
        let frecency = FrecencyStore::new();
        let mut filter = ProviderFilter::all();
        filter.set_prefix(prefix);
        let _ = pm.search_with_frecency("firefox", 10, &filter, &frecency, 0.0, None);
        calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn expensive_dynamic_provider_skipped_in_auto_mode() {
        assert_eq!(
            run_spy_search(false, None),
            0,
            "a runs_in_auto_mode()=false provider must not run on a bare auto-mode query"
        );
    }

    #[test]
    fn expensive_dynamic_provider_runs_when_prefix_active() {
        assert_eq!(
            run_spy_search(false, Some(ProviderType::Plugin("filesearch".into()))),
            1,
            "the provider must run when its own prefix is active"
        );
    }

    #[test]
    fn cheap_dynamic_provider_runs_in_auto_mode() {
        assert_eq!(
            run_spy_search(true, None),
            1,
            "a runs_in_auto_mode()=true provider must run on a bare auto-mode query"
        );
    }

    // =========================================================================
    // Tests for behavior introduced in the v2 C-ABI demolition (commit ae4a903)
    // =========================================================================

    /// Provider impl that overrides every trait method to verify customization
    /// flows through `ProviderManager::available_providers()` and submenu/action
    /// dispatch.
    struct RichMockProvider {
        type_id: String,
        items: Vec<LaunchItem>,
        submenu: Vec<LaunchItem>,
        action_handled_for: Option<String>,
    }

    impl Provider for RichMockProvider {
        fn name(&self) -> &str {
            "Rich"
        }
        fn provider_type(&self) -> ProviderType {
            ProviderType::Plugin(self.type_id.clone())
        }
        fn refresh(&mut self) {}
        fn items(&self) -> &[LaunchItem] {
            &self.items
        }
        fn prefix(&self) -> Option<&str> {
            Some(":rich")
        }
        fn icon(&self) -> &str {
            "rich-icon"
        }
        fn position(&self) -> ProviderPosition {
            ProviderPosition::Widget
        }
        fn priority(&self) -> u32 {
            42
        }
        fn tab_label(&self) -> Option<&str> {
            Some("Rich")
        }
        fn search_noun(&self) -> Option<&str> {
            Some("rich things")
        }
        fn submenu_actions(&self, _data: &str) -> Vec<LaunchItem> {
            self.submenu.clone()
        }
        fn execute_action(&self, command: &str) -> bool {
            self.action_handled_for
                .as_ref()
                .map(|prefix| command.starts_with(prefix.as_str()))
                .unwrap_or(false)
        }
    }

    #[test]
    fn provider_trait_default_methods_return_documented_values() {
        // MockProvider does not override any optional method; defaults must hold.
        let p = MockProvider::new("M", ProviderType::Application);
        assert_eq!(p.prefix(), None);
        assert_eq!(p.icon(), "application-x-addon");
        assert_eq!(p.position(), ProviderPosition::Normal);
        assert_eq!(p.priority(), 0);
        assert_eq!(p.tab_label(), None);
        assert_eq!(p.search_noun(), None);
        assert!(p.submenu_actions("anything").is_empty());
        assert!(!p.execute_action("MOCK:anything"));
    }

    #[test]
    fn provider_position_as_str_matches_ipc_strings() {
        assert_eq!(ProviderPosition::Normal.as_str(), "normal");
        assert_eq!(ProviderPosition::Widget.as_str(), "widget");
    }

    #[test]
    fn provider_type_from_str_maps_power_aliases() {
        // v2 renamed `sys` -> `power`. `sys` and `system` are kept as aliases
        // for muscle memory; they must all resolve to Plugin("power"), never
        // Plugin("sys") or Plugin("system").
        use std::str::FromStr;
        assert_eq!(
            ProviderType::from_str("power").unwrap(),
            ProviderType::Plugin("power".into())
        );
        assert_eq!(
            ProviderType::from_str("sys").unwrap(),
            ProviderType::Plugin("power".into())
        );
        assert_eq!(
            ProviderType::from_str("system").unwrap(),
            ProviderType::Plugin("power".into())
        );
    }

    #[test]
    fn provider_type_from_str_maps_systemd_aliases() {
        // systemd provider's type_id is `uuctl` (CLI back-compat from pre-v2).
        // Both `uuctl` and `systemd` must resolve to Plugin("uuctl").
        use std::str::FromStr;
        assert_eq!(
            ProviderType::from_str("uuctl").unwrap(),
            ProviderType::Plugin("uuctl".into())
        );
        assert_eq!(
            ProviderType::from_str("systemd").unwrap(),
            ProviderType::Plugin("uuctl".into())
        );
    }

    #[test]
    fn provider_type_from_str_accepts_plural_aliases() {
        // After the demolition, FromStr accepts both singular and plural aliases.
        use std::str::FromStr;
        assert_eq!(
            ProviderType::from_str("app").unwrap(),
            ProviderType::Application
        );
        assert_eq!(
            ProviderType::from_str("apps").unwrap(),
            ProviderType::Application
        );
        assert_eq!(
            ProviderType::from_str("application").unwrap(),
            ProviderType::Application
        );
        assert_eq!(
            ProviderType::from_str("applications").unwrap(),
            ProviderType::Application
        );
        assert_eq!(
            ProviderType::from_str("cmd").unwrap(),
            ProviderType::Command
        );
        assert_eq!(
            ProviderType::from_str("cmds").unwrap(),
            ProviderType::Command
        );
        assert_eq!(
            ProviderType::from_str("command").unwrap(),
            ProviderType::Command
        );
        assert_eq!(
            ProviderType::from_str("commands").unwrap(),
            ProviderType::Command
        );
        assert_eq!(
            ProviderType::from_str("dmenu").unwrap(),
            ProviderType::Dmenu
        );
        // Anything unknown becomes Plugin(s) — preserves user-defined provider IDs.
        assert_eq!(
            ProviderType::from_str("bookmarks").unwrap(),
            ProviderType::Plugin("bookmarks".into())
        );
        assert_eq!(
            ProviderType::from_str("uuctl").unwrap(),
            ProviderType::Plugin("uuctl".into())
        );
    }

    #[test]
    fn item_source_from_str_maps_unknown_to_core() {
        // NativePlugin variant is gone; unknown strings (including the old
        // "native_plugin" tag from pre-2.0 daemons) decode to Core.
        use std::str::FromStr;
        assert_eq!(ItemSource::from_str("core"), Ok(ItemSource::Core));
        assert_eq!(
            ItemSource::from_str("script_plugin"),
            Ok(ItemSource::ScriptPlugin)
        );
        assert_eq!(ItemSource::from_str("native_plugin"), Ok(ItemSource::Core));
        assert_eq!(ItemSource::from_str(""), Ok(ItemSource::Core));
        assert_eq!(ItemSource::from_str("anything-else"), Ok(ItemSource::Core));
    }

    #[test]
    fn item_source_as_str_only_emits_supported_variants() {
        assert_eq!(ItemSource::Core.as_str(), "core");
        assert_eq!(ItemSource::ScriptPlugin.as_str(), "script_plugin");
    }

    #[test]
    fn add_provider_refreshes_and_appends() {
        let mut pm = ProviderManager::new(Vec::new(), Vec::new());
        assert_eq!(pm.available_provider_types().len(), 0);

        let prov = MockProvider::new("Late", ProviderType::Plugin("late".into()));
        pm.add_provider(Box::new(prov));

        let types = pm.available_provider_types();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0], ProviderType::Plugin("late".into()));
        // add_provider must call refresh() — checked indirectly by the impl
        // contract; refresh_count on MockProvider was bumped, but we can't
        // peek through the Box<dyn>. The public observable is that items()
        // is callable without panic, which we exercise here.
        assert!(pm.available_providers()[0].id == "late");
    }

    #[test]
    fn available_providers_uses_trait_overrides_for_plugin_type() {
        let prov = RichMockProvider {
            type_id: "rich".into(),
            items: Vec::new(),
            submenu: Vec::new(),
            action_handled_for: None,
        };
        let pm = ProviderManager::new(vec![Box::new(prov)], Vec::new());
        let descs = pm.available_providers();
        assert_eq!(descs.len(), 1);
        let d = &descs[0];
        assert_eq!(d.id, "rich");
        assert_eq!(d.prefix.as_deref(), Some(":rich"));
        assert_eq!(d.icon, "rich-icon");
        assert_eq!(d.position, "widget");
        assert_eq!(d.tab_label.as_deref(), Some("Rich"));
        assert_eq!(d.search_noun.as_deref(), Some("rich things"));
    }

    #[test]
    fn query_submenu_actions_returns_some_when_provider_matches_and_has_actions() {
        let prov = RichMockProvider {
            type_id: "uuctl".into(),
            items: Vec::new(),
            submenu: vec![make_item(
                "start",
                "Start service",
                ProviderType::Plugin("uuctl".into()),
            )],
            action_handled_for: None,
        };
        let pm = ProviderManager::new(vec![Box::new(prov)], Vec::new());

        let result = pm.query_submenu_actions("uuctl", "foo.service:true", "foo");
        assert!(
            result.is_some(),
            "expected Some when provider matches and returns actions"
        );
        let (display, actions) = result.unwrap();
        assert_eq!(display, "foo");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Start service");
    }

    #[test]
    fn query_submenu_actions_returns_none_when_no_provider_matches() {
        let prov = RichMockProvider {
            type_id: "uuctl".into(),
            items: Vec::new(),
            submenu: vec![make_item("x", "x", ProviderType::Plugin("uuctl".into()))],
            action_handled_for: None,
        };
        let pm = ProviderManager::new(vec![Box::new(prov)], Vec::new());
        assert!(
            pm.query_submenu_actions("does-not-exist", "data", "name")
                .is_none()
        );
    }

    #[test]
    fn query_submenu_actions_returns_none_when_provider_returns_empty_actions() {
        let prov = RichMockProvider {
            type_id: "uuctl".into(),
            items: Vec::new(),
            submenu: Vec::new(), // matching provider but no actions
            action_handled_for: None,
        };
        let pm = ProviderManager::new(vec![Box::new(prov)], Vec::new());
        assert!(pm.query_submenu_actions("uuctl", "data", "name").is_none());
    }

    #[test]
    fn execute_plugin_action_returns_true_when_static_provider_handles() {
        let prov = RichMockProvider {
            type_id: "pomodoro".into(),
            items: Vec::new(),
            submenu: Vec::new(),
            action_handled_for: Some("POMODORO:".into()),
        };
        let pm = ProviderManager::new(vec![Box::new(prov)], Vec::new());
        assert!(pm.execute_plugin_action("POMODORO:start"));
    }

    #[test]
    fn execute_plugin_action_returns_true_when_dynamic_provider_handles() {
        struct DynStub {
            handles: String,
        }
        impl DynamicProvider for DynStub {
            fn name(&self) -> &str {
                "dyn"
            }
            fn provider_type(&self) -> ProviderType {
                ProviderType::Plugin("dyn".into())
            }
            fn query(&self, _q: &str) -> Vec<LaunchItem> {
                Vec::new()
            }
            fn priority(&self) -> u32 {
                0
            }
            fn execute_action(&self, command: &str) -> bool {
                command.starts_with(&self.handles)
            }
        }

        let pm = ProviderManager::new(
            Vec::new(),
            vec![Box::new(DynStub {
                handles: "DYN:".into(),
            })],
        );
        assert!(pm.execute_plugin_action("DYN:thing"));
    }

    #[test]
    fn available_providers_includes_dynamic_providers() {
        // Regression guard (2.0.1): dynamic providers must surface in the
        // diagnostic list so `owlry doctor` and `owlry providers` can report
        // them. Pre-2.0.1, only static providers were enumerated and the
        // dynamic ones were silently absent despite running fine.
        struct DynRich;
        impl DynamicProvider for DynRich {
            fn name(&self) -> &str {
                "Rich Dynamic"
            }
            fn provider_type(&self) -> ProviderType {
                ProviderType::Plugin("rich-dyn".into())
            }
            fn query(&self, _q: &str) -> Vec<LaunchItem> {
                Vec::new()
            }
            fn priority(&self) -> u32 {
                7_000
            }
            fn prefix(&self) -> Option<&str> {
                Some(":rdyn")
            }
            fn icon(&self) -> &str {
                "rich-dynamic-icon"
            }
            fn tab_label(&self) -> Option<&str> {
                Some("Rich")
            }
            fn search_noun(&self) -> Option<&str> {
                Some("rich queries")
            }
        }

        let pm = ProviderManager::new(Vec::new(), vec![Box::new(DynRich)]);
        let descs = pm.available_providers();
        assert_eq!(descs.len(), 1);
        let d = &descs[0];
        assert_eq!(d.id, "rich-dyn");
        assert_eq!(d.name, "Rich Dynamic");
        assert_eq!(d.prefix.as_deref(), Some(":rdyn"));
        assert_eq!(d.icon, "rich-dynamic-icon");
        assert_eq!(d.position, "normal");
        assert_eq!(d.tab_label.as_deref(), Some("Rich"));
        assert_eq!(d.search_noun.as_deref(), Some("rich queries"));
        assert_eq!(d.kind, ProviderKind::Dynamic);
    }

    #[test]
    fn available_providers_marks_static_vs_dynamic_kind() {
        // Static providers come from `providers`, dynamic from `builtin_dynamic`;
        // `owlry doctor` distinguishes them by the descriptor's `kind`.
        struct StaticStub;
        impl Provider for StaticStub {
            fn name(&self) -> &str {
                "Static Stub"
            }
            fn provider_type(&self) -> ProviderType {
                ProviderType::Application
            }
            fn refresh(&mut self) {}
            fn items(&self) -> &[LaunchItem] {
                &[]
            }
        }
        struct DynStub;
        impl DynamicProvider for DynStub {
            fn name(&self) -> &str {
                "Dyn Stub"
            }
            fn provider_type(&self) -> ProviderType {
                ProviderType::Plugin("dyn-stub".into())
            }
            fn query(&self, _q: &str) -> Vec<LaunchItem> {
                Vec::new()
            }
            fn priority(&self) -> u32 {
                5_000
            }
        }

        let pm = ProviderManager::new(vec![Box::new(StaticStub)], vec![Box::new(DynStub)]);
        let descs = pm.available_providers();
        let stat = descs.iter().find(|d| d.id == "app").unwrap();
        let dyn_ = descs.iter().find(|d| d.id == "dyn-stub").unwrap();
        assert_eq!(stat.kind, ProviderKind::Static);
        assert_eq!(dyn_.kind, ProviderKind::Dynamic);
    }

    #[test]
    fn available_provider_types_includes_dynamic_providers() {
        struct DynStub;
        impl DynamicProvider for DynStub {
            fn name(&self) -> &str {
                "stub"
            }
            fn provider_type(&self) -> ProviderType {
                ProviderType::Plugin("stub".into())
            }
            fn query(&self, _q: &str) -> Vec<LaunchItem> {
                Vec::new()
            }
            fn priority(&self) -> u32 {
                0
            }
        }

        let mock = MockProvider::new("Apps", ProviderType::Application);
        let pm = ProviderManager::new(vec![Box::new(mock)], vec![Box::new(DynStub)]);
        let types = pm.available_provider_types();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&ProviderType::Application));
        assert!(types.contains(&ProviderType::Plugin("stub".into())));
    }

    #[test]
    fn dynamic_provider_trait_defaults_return_documented_values() {
        // 2.0.1 added prefix/icon/tab_label/search_noun as defaulted methods
        // on DynamicProvider so the trait can describe its own UI metadata
        // without falling back to a hardcoded match table.
        struct Minimal;
        impl DynamicProvider for Minimal {
            fn name(&self) -> &str {
                "m"
            }
            fn provider_type(&self) -> ProviderType {
                ProviderType::Plugin("m".into())
            }
            fn query(&self, _: &str) -> Vec<LaunchItem> {
                Vec::new()
            }
            fn priority(&self) -> u32 {
                0
            }
        }
        let m = Minimal;
        assert_eq!(m.prefix(), None);
        assert_eq!(m.icon(), "application-x-addon");
        assert_eq!(m.tab_label(), None);
        assert_eq!(m.search_noun(), None);
    }

    #[test]
    fn execute_plugin_action_returns_false_when_nothing_handles() {
        let prov = RichMockProvider {
            type_id: "x".into(),
            items: Vec::new(),
            submenu: Vec::new(),
            action_handled_for: Some("X:".into()),
        };
        let pm = ProviderManager::new(vec![Box::new(prov)], Vec::new());
        assert!(!pm.execute_plugin_action("UNRELATED:command"));
    }

    #[test]
    fn provider_manager_new_with_no_providers_does_not_panic() {
        // Regression guard: the daemon may be configured to disable every
        // provider; construction must still succeed.
        let pm = ProviderManager::new(Vec::new(), Vec::new());
        assert_eq!(pm.available_providers().len(), 0);
        assert!(!pm.execute_plugin_action("ANYTHING:foo"));
        assert!(pm.query_submenu_actions("anything", "data", "n").is_none());
    }
}
