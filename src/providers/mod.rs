mod application;
mod bookmarks;
mod calculator;
mod clipboard;
mod command;
mod dmenu;
mod emoji;
mod files;
mod scripts;
mod ssh;
mod system;
mod uuctl;
mod websearch;

pub use application::ApplicationProvider;
pub use bookmarks::BookmarksProvider;
pub use calculator::CalculatorProvider;
pub use clipboard::ClipboardProvider;
pub use command::CommandProvider;
pub use dmenu::DmenuProvider;
pub use emoji::EmojiProvider;
pub use files::FileSearchProvider;
pub use scripts::ScriptsProvider;
pub use ssh::SshProvider;
pub use system::SystemProvider;
pub use uuctl::UuctlProvider;
pub use websearch::WebSearchProvider;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use log::info;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderType {
    Application,
    Bookmarks,
    Calculator,
    Clipboard,
    Command,
    Dmenu,
    Emoji,
    Files,
    Scripts,
    Ssh,
    System,
    Uuctl,
    WebSearch,
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "app" | "apps" | "application" | "applications" => Ok(ProviderType::Application),
            "bookmark" | "bookmarks" | "bm" => Ok(ProviderType::Bookmarks),
            "calc" | "calculator" => Ok(ProviderType::Calculator),
            "clip" | "clipboard" => Ok(ProviderType::Clipboard),
            "cmd" | "command" | "commands" => Ok(ProviderType::Command),
            "dmenu" => Ok(ProviderType::Dmenu),
            "emoji" | "emojis" => Ok(ProviderType::Emoji),
            "file" | "files" | "find" => Ok(ProviderType::Files),
            "script" | "scripts" => Ok(ProviderType::Scripts),
            "ssh" => Ok(ProviderType::Ssh),
            "sys" | "system" | "power" => Ok(ProviderType::System),
            "uuctl" => Ok(ProviderType::Uuctl),
            "web" | "websearch" | "search" => Ok(ProviderType::WebSearch),
            _ => Err(format!(
                "Unknown provider: '{}'. Valid: app, bookmark, calc, clip, cmd, emoji, file, script, ssh, sys, web",
                s
            )),
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Application => write!(f, "app"),
            ProviderType::Bookmarks => write!(f, "bookmark"),
            ProviderType::Calculator => write!(f, "calc"),
            ProviderType::Clipboard => write!(f, "clip"),
            ProviderType::Command => write!(f, "cmd"),
            ProviderType::Dmenu => write!(f, "dmenu"),
            ProviderType::Emoji => write!(f, "emoji"),
            ProviderType::Files => write!(f, "file"),
            ProviderType::Scripts => write!(f, "script"),
            ProviderType::Ssh => write!(f, "ssh"),
            ProviderType::System => write!(f, "sys"),
            ProviderType::Uuctl => write!(f, "uuctl"),
            ProviderType::WebSearch => write!(f, "web"),
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
    providers: Vec<Box<dyn Provider>>,
    calculator: CalculatorProvider,
    websearch: WebSearchProvider,
    filesearch: FileSearchProvider,
    matcher: SkimMatcherV2,
}

impl ProviderManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_search_engine("duckduckgo")
    }

    pub fn with_search_engine(search_engine: &str) -> Self {
        Self::with_config(search_engine, "kitty")
    }

    pub fn with_config(search_engine: &str, terminal: &str) -> Self {
        let mut manager = Self {
            providers: Vec::new(),
            calculator: CalculatorProvider::new(),
            websearch: WebSearchProvider::with_engine(search_engine),
            filesearch: FileSearchProvider::new(),
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
            // Normal mode: use all standard providers
            manager.providers.push(Box::new(ApplicationProvider::new()));
            manager.providers.push(Box::new(CommandProvider::new()));
            manager.providers.push(Box::new(UuctlProvider::new()));

            // New providers
            manager.providers.push(Box::new(SystemProvider::new()));
            manager.providers.push(Box::new(SshProvider::with_terminal(terminal)));
            manager.providers.push(Box::new(ClipboardProvider::new()));
            manager.providers.push(Box::new(BookmarksProvider::new()));
            manager.providers.push(Box::new(EmojiProvider::new()));
            manager.providers.push(Box::new(ScriptsProvider::new()));
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
        for provider in &mut self.providers {
            provider.refresh();
            info!(
                "Provider '{}' loaded {} items",
                provider.name(),
                provider.items().len()
            );
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

    /// Search with frecency boosting and calculator support
    pub fn search_with_frecency(
        &mut self,
        query: &str,
        max_results: usize,
        filter: &crate::filter::ProviderFilter,
        frecency: &FrecencyStore,
        frecency_weight: f64,
    ) -> Vec<(LaunchItem, i64)> {
        let mut results: Vec<(LaunchItem, i64)> = Vec::new();

        // Check for calculator query (= or calc prefix)
        if CalculatorProvider::is_calculator_query(query) {
            if let Some(calc_result) = self.calculator.evaluate(query) {
                // Calculator results get a high score to appear first
                results.push((calc_result, 10000));
            }
        }
        // Also check for raw expression when in :calc filter mode
        else if filter.active_prefix() == Some(ProviderType::Calculator)
            && CalculatorProvider::looks_like_expression(query)
        {
            if let Some(calc_result) = self.calculator.evaluate_raw(query) {
                results.push((calc_result, 10000));
            }
        }

        // Check for web search query
        if WebSearchProvider::is_websearch_query(query) {
            if let Some(web_result) = self.websearch.evaluate(query) {
                // Web search results get a high score to appear first
                results.push((web_result, 9000));
            }
        }
        // Also check for raw query when in :web filter mode
        else if filter.active_prefix() == Some(ProviderType::WebSearch) && !query.is_empty() {
            if let Some(web_result) = self.websearch.evaluate_raw(query) {
                results.push((web_result, 9000));
            }
        }

        // Check for file search query
        if FileSearchProvider::is_file_query(query) {
            let file_results = self.filesearch.evaluate(query);
            for (idx, item) in file_results.into_iter().enumerate() {
                // Score decreases for each result to maintain order
                results.push((item, 8000 - idx as i64));
            }
        }
        // Also check for raw query when in :file filter mode
        else if filter.active_prefix() == Some(ProviderType::Files) && !query.is_empty() {
            let file_results = self.filesearch.evaluate_raw(query);
            for (idx, item) in file_results.into_iter().enumerate() {
                results.push((item, 8000 - idx as i64));
            }
        }

        // Empty query (after checking special providers) - return frecency-sorted items
        if query.is_empty() {
            let mut items: Vec<(LaunchItem, i64)> = self
                .providers
                .iter()
                .filter(|p| filter.is_active(p.provider_type()))
                .flat_map(|p| p.items().iter().cloned())
                .map(|item| {
                    let frecency_score = frecency.get_score(&item.id);
                    let boosted = (frecency_score * frecency_weight * 100.0) as i64;
                    (item, boosted)
                })
                .collect();

            items.sort_by(|a, b| b.1.cmp(&a.1));
            items.truncate(max_results);
            return items;
        }

        // Regular search with frecency boost
        let search_results: Vec<(LaunchItem, i64)> = self
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

                    let base_score = match (name_score, desc_score) {
                        (Some(n), Some(d)) => Some(n.max(d)),
                        (Some(n), None) => Some(n),
                        (None, Some(d)) => Some(d / 2),
                        (None, None) => None,
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
        results
    }

    /// Get all available provider types (for UI tabs)
    #[allow(dead_code)]
    pub fn available_providers(&self) -> Vec<ProviderType> {
        self.providers.iter().map(|p| p.provider_type()).collect()
    }
}
