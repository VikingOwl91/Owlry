mod application;
mod bookmarks;
mod calculator;
mod clipboard;
mod command;
mod dmenu;
mod emoji;
mod files;
mod media;
mod pomodoro;
mod scripts;
mod ssh;
mod system;
mod uuctl;
mod weather;
mod websearch;

pub use application::ApplicationProvider;
pub use bookmarks::BookmarksProvider;
pub use calculator::CalculatorProvider;
pub use clipboard::ClipboardProvider;
pub use command::CommandProvider;
pub use dmenu::DmenuProvider;
pub use emoji::EmojiProvider;
pub use files::FileSearchProvider;
pub use media::MediaProvider;
pub use pomodoro::{PomodoroConfig, PomodoroProvider};
pub use scripts::ScriptsProvider;
pub use ssh::SshProvider;
pub use system::SystemProvider;
pub use uuctl::UuctlProvider;
pub use weather::{WeatherConfig, WeatherProvider, WeatherProviderType};
pub use websearch::WebSearchProvider;

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
    MediaPlayer,
    Pomodoro,
    Scripts,
    Ssh,
    System,
    Uuctl,
    Weather,
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
            "media" | "mpris" | "player" => Ok(ProviderType::MediaPlayer),
            "pomo" | "pomodoro" | "timer" => Ok(ProviderType::Pomodoro),
            "script" | "scripts" => Ok(ProviderType::Scripts),
            "ssh" => Ok(ProviderType::Ssh),
            "sys" | "system" | "power" => Ok(ProviderType::System),
            "uuctl" => Ok(ProviderType::Uuctl),
            "weather" => Ok(ProviderType::Weather),
            "web" | "websearch" | "search" => Ok(ProviderType::WebSearch),
            _ => Err(format!(
                "Unknown provider: '{}'. Valid: app, bookmark, calc, clip, cmd, emoji, file, media, pomo, script, ssh, sys, weather, web",
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
            ProviderType::MediaPlayer => write!(f, "media"),
            ProviderType::Pomodoro => write!(f, "pomo"),
            ProviderType::Scripts => write!(f, "script"),
            ProviderType::Ssh => write!(f, "ssh"),
            ProviderType::System => write!(f, "sys"),
            ProviderType::Uuctl => write!(f, "uuctl"),
            ProviderType::Weather => write!(f, "weather"),
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
    // Widget providers (optional, controlled by config)
    media: Option<MediaProvider>,
    weather: Option<WeatherProvider>,
    pomodoro: Option<PomodoroProvider>,
    matcher: SkimMatcherV2,
}

impl ProviderManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_search_engine("duckduckgo")
    }

    pub fn with_search_engine(search_engine: &str) -> Self {
        // Use xterm as fallback - it's the universal cockroach of terminals
        // In practice, app.rs passes the detected/configured terminal
        Self::with_config(search_engine, "xterm", true, None, None)
    }

    pub fn with_config(
        search_engine: &str,
        terminal: &str,
        media_enabled: bool,
        weather_config: Option<WeatherConfig>,
        pomodoro_config: Option<PomodoroConfig>,
    ) -> Self {
        let mut manager = Self {
            providers: Vec::new(),
            calculator: CalculatorProvider::new(),
            websearch: WebSearchProvider::with_engine(search_engine),
            filesearch: FileSearchProvider::new(),
            media: if media_enabled { Some(MediaProvider::new()) } else { None },
            weather: weather_config.map(WeatherProvider::new),
            pomodoro: pomodoro_config.map(PomodoroProvider::new),
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

        // Refresh widget providers
        if let Some(ref mut media) = self.media {
            media.refresh();
            info!("Media widget loaded {} items", media.items().len());
        }
        if let Some(ref mut weather) = self.weather {
            weather.refresh();
            info!("Weather widget loaded {} items", weather.items().len());
        }
        if let Some(ref mut pomodoro) = self.pomodoro {
            pomodoro.refresh();
            info!("Pomodoro widget loaded {} items", pomodoro.items().len());
        }
    }

    /// Get mutable reference to pomodoro provider (for handling actions)
    pub fn pomodoro_mut(&mut self) -> Option<&mut PomodoroProvider> {
        self.pomodoro.as_mut()
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

    /// Search with frecency boosting, calculator support, and tag filtering
    pub fn search_with_frecency(
        &mut self,
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

        // Add widget items first (highest priority) - only when no specific filter is active
        if filter.active_prefix().is_none() {
            // Weather widget (score 12000) - shown at very top
            if let Some(ref weather) = self.weather {
                for item in weather.items() {
                    results.push((item.clone(), 12000));
                }
            }

            // Pomodoro widget (scores 11500-11502)
            if let Some(ref pomodoro) = self.pomodoro {
                for (idx, item) in pomodoro.items().iter().enumerate() {
                    results.push((item.clone(), 11502 - idx as i64));
                }
            }

            // Media widget (scores 11000-11003)
            if let Some(ref media) = self.media {
                for (idx, item) in media.items().iter().enumerate() {
                    results.push((item.clone(), 11003 - idx as i64));
                }
            }
        }

        // Check for calculator query (= or calc prefix)
        if CalculatorProvider::is_calculator_query(query) {
            if let Some(calc_result) = self.calculator.evaluate(query) {
                #[cfg(feature = "dev-logging")]
                debug!("[Search] Calculator result: {}", calc_result.name);
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
            #[cfg(feature = "dev-logging")]
            debug!("[Search] File search returned {} results", file_results.len());
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
                    if let Some(tag) = tag_filter {
                        if !item.tags.iter().any(|t| t.to_lowercase().contains(tag)) {
                            return None;
                        }
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
}
