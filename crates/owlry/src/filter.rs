use std::collections::HashSet;

#[cfg(feature = "dev-logging")]
use log::debug;

use crate::config::ProvidersConfig;
use crate::providers::ProviderType;

/// Tracks which providers are enabled and handles prefix-based filtering
#[derive(Debug, Clone)]
pub struct ProviderFilter {
    enabled: HashSet<ProviderType>,
    active_prefix: Option<ProviderType>,
    /// When true, `is_active`/`is_enabled` accept any provider type
    /// (unless a prefix narrows the scope). Used by `all()` so that
    /// dynamically loaded plugins are accepted without being listed.
    accept_all: bool,
}

/// Result of parsing a query for prefix syntax
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub prefix: Option<ProviderType>,
    pub tag_filter: Option<String>,
    pub query: String,
}

impl ProviderFilter {
    /// Create filter from CLI args and config.
    ///
    /// `tabs` is `general.tabs` from config and drives which provider tabs are
    /// shown in the UI when no explicit CLI mode is active. It has no effect on
    /// query routing: when no CLI mode is set, `accept_all=true` causes
    /// `is_active()` to return `true` for every provider regardless.
    pub fn new(
        cli_mode: Option<ProviderType>,
        cli_providers: Option<Vec<ProviderType>>,
        config_providers: &ProvidersConfig,
        tabs: &[String],
    ) -> Self {
        let accept_all = cli_mode.is_none() && cli_providers.is_none();

        let enabled = if let Some(mode) = cli_mode {
            // --mode overrides everything: single provider
            HashSet::from([mode])
        } else if let Some(providers) = cli_providers {
            // --providers overrides config
            providers.into_iter().collect()
        } else {
            // No CLI restriction: accept_all=true, so is_active() returns true for
            // everything. Build the enabled set only for UI tab display, driven by
            // general.tabs. Falls back to Application + Command if tabs is empty.
            let mut set: HashSet<ProviderType> = tabs
                .iter()
                .map(|s| Self::mode_string_to_provider_type(s))
                .collect();
            if set.is_empty() {
                if config_providers.applications {
                    set.insert(ProviderType::Application);
                }
                if config_providers.commands {
                    set.insert(ProviderType::Command);
                }
                if set.is_empty() {
                    set.insert(ProviderType::Application);
                }
            }
            set
        };

        let filter = Self {
            enabled,
            active_prefix: None,
            accept_all,
        };

        #[cfg(feature = "dev-logging")]
        debug!(
            "[Filter] Created with enabled providers: {:?}",
            filter.enabled
        );

        filter
    }

    /// Default filter: apps only
    #[allow(dead_code)]
    pub fn apps_only() -> Self {
        Self {
            enabled: HashSet::from([ProviderType::Application]),
            active_prefix: None,
            accept_all: false,
        }
    }

    /// Toggle a provider on/off. Clears accept_all so the enabled set is
    /// actually used for routing — use restore_all_mode() to go back to All.
    pub fn toggle(&mut self, provider: ProviderType) {
        if self.enabled.contains(&provider) {
            self.enabled.remove(&provider);
            // Ensure at least one provider is always enabled
            if self.enabled.is_empty() {
                self.enabled.insert(ProviderType::Application);
            }
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] Toggled OFF {:?}, enabled: {:?}",
                provider, self.enabled
            );
        } else {
            #[cfg(feature = "dev-logging")]
            let provider_debug = format!("{:?}", provider);
            self.enabled.insert(provider);
            #[cfg(feature = "dev-logging")]
            debug!(
                "[Filter] Toggled ON {}, enabled: {:?}",
                provider_debug, self.enabled
            );
        }
        self.accept_all = false;
    }

    /// Enable a specific provider
    pub fn enable(&mut self, provider: ProviderType) {
        self.enabled.insert(provider);
    }

    /// Disable a specific provider (ensures at least one remains)
    pub fn disable(&mut self, provider: ProviderType) {
        self.enabled.remove(&provider);
        if self.enabled.is_empty() {
            self.enabled.insert(ProviderType::Application);
        }
    }

    /// Set to single provider mode
    pub fn set_single_mode(&mut self, provider: ProviderType) {
        self.enabled.clear();
        self.enabled.insert(provider);
        self.accept_all = false;
    }

    /// Restore accept-all mode (used when cycling back to the "All" tab).
    pub fn restore_all_mode(&mut self) {
        self.accept_all = true;
    }

    /// Set prefix mode (from :app, :cmd, etc.)
    pub fn set_prefix(&mut self, prefix: Option<ProviderType>) {
        #[cfg(feature = "dev-logging")]
        if self.active_prefix != prefix {
            debug!(
                "[Filter] Prefix changed: {:?} -> {:?}",
                self.active_prefix, prefix
            );
        }
        self.active_prefix = prefix;
    }

    /// Check if a provider should be searched
    pub fn is_active(&self, provider: ProviderType) -> bool {
        if let Some(ref prefix) = self.active_prefix {
            &provider == prefix
        } else if self.accept_all {
            true
        } else {
            self.enabled.contains(&provider)
        }
    }

    /// Check if provider is in enabled set (ignoring prefix)
    pub fn is_enabled(&self, provider: ProviderType) -> bool {
        self.accept_all || self.enabled.contains(&provider)
    }

    /// Whether this filter accepts all provider types
    pub fn is_accept_all(&self) -> bool {
        self.accept_all
    }

    /// Get current active prefix if any
    #[allow(dead_code)]
    pub fn active_prefix(&self) -> Option<ProviderType> {
        self.active_prefix.clone()
    }

    /// Parse query for prefix syntax
    /// Prefixes map to Plugin(type_id) for plugin providers
    pub fn parse_query(query: &str) -> ParsedQuery {
        let trimmed = query.trim_start();

        // Check for tag filter pattern: ":tag:XXX query" or ":tag:XXX"
        if let Some(rest) = trimmed.strip_prefix(":tag:") {
            // Find the end of the tag (space or end of string)
            if let Some(space_idx) = rest.find(' ') {
                let tag = rest[..space_idx].to_lowercase();
                let query_part = rest[space_idx + 1..].to_string();
                #[cfg(feature = "dev-logging")]
                debug!(
                    "[Filter] parse_query({:?}) -> tag={:?}, query={:?}",
                    query, tag, query_part
                );
                return ParsedQuery {
                    prefix: None,
                    tag_filter: Some(tag),
                    query: query_part,
                };
            } else {
                // Just the tag, no query yet
                let tag = rest.to_lowercase();
                return ParsedQuery {
                    prefix: None,
                    tag_filter: Some(tag),
                    query: String::new(),
                };
            }
        }

        // `/` is a shorthand alias for the `:file` prefix. `/foo` and `/ foo`
        // both route to file search with term "foo"; a bare `/` is a partial
        // prefix with an empty query. This produces an identical ParsedQuery to
        // `:file foo`, so the daemon path is shared.
        if let Some(rest) = trimmed.strip_prefix('/') {
            return ParsedQuery {
                prefix: Some(ProviderType::Plugin("filesearch".to_string())),
                tag_filter: None,
                query: rest.trim_start().to_string(),
            };
        }

        // Core prefixes — each entry is tried as ":name " (full) and ":name" (partial)
        let core_prefixes: &[(&str, ProviderType)] = &[
            ("app", ProviderType::Application),
            ("apps", ProviderType::Application),
            ("cmd", ProviderType::Command),
            ("command", ProviderType::Command),
        ];

        // Plugin prefixes — each entry maps to a plugin type_id
        let plugin_prefixes: &[(&str, &str)] = &[
            ("bm", "bookmarks"),
            ("bookmark", "bookmarks"),
            ("bookmarks", "bookmarks"),
            ("calc", "calc"),
            ("calculator", "calc"),
            ("clip", "clipboard"),
            ("clipboard", "clipboard"),
            ("emoji", "emoji"),
            ("emojis", "emoji"),
            ("rofimoji", "rofimoji"),
            ("chars", "rofimoji"),
            ("characters", "rofimoji"),
            ("kaomoji", "rofimoji"),
            ("file", "filesearch"),
            ("files", "filesearch"),
            ("find", "filesearch"),
            ("script", "scripts"),
            ("scripts", "scripts"),
            ("ssh", "ssh"),
            ("sys", "power"),
            ("system", "power"),
            ("power", "power"),
            ("uuctl", "uuctl"),
            ("systemd", "uuctl"),
            ("web", "websearch"),
            ("search", "websearch"),
            ("config", "config"),
            ("settings", "config"),
            ("conv", "conv"),
            ("converter", "conv"),
        ];

        // Single-pass: try each core prefix as both full (":name query") and partial (":name")
        for (name, provider) in core_prefixes {
            let with_space = format!(":{} ", name);
            if let Some(rest) = trimmed.strip_prefix(with_space.as_str()) {
                #[cfg(feature = "dev-logging")]
                debug!(
                    "[Filter] parse_query({:?}) -> prefix={:?}, query={:?}",
                    query, provider, rest
                );
                return ParsedQuery {
                    prefix: Some(provider.clone()),
                    tag_filter: None,
                    query: rest.to_string(),
                };
            }
            let exact = format!(":{}", name);
            if trimmed == exact {
                #[cfg(feature = "dev-logging")]
                debug!(
                    "[Filter] parse_query({:?}) -> partial prefix {:?}",
                    query, provider
                );
                return ParsedQuery {
                    prefix: Some(provider.clone()),
                    tag_filter: None,
                    query: String::new(),
                };
            }
        }

        // Single-pass: try each plugin prefix as both full and partial
        for (name, type_id) in plugin_prefixes {
            let with_space = format!(":{} ", name);
            if let Some(rest) = trimmed.strip_prefix(with_space.as_str()) {
                let provider = ProviderType::Plugin(type_id.to_string());
                #[cfg(feature = "dev-logging")]
                debug!(
                    "[Filter] parse_query({:?}) -> prefix={:?}, query={:?}",
                    query, provider, rest
                );
                return ParsedQuery {
                    prefix: Some(provider),
                    tag_filter: None,
                    query: rest.to_string(),
                };
            }
            let exact = format!(":{}", name);
            if trimmed == exact {
                let provider = ProviderType::Plugin(type_id.to_string());
                #[cfg(feature = "dev-logging")]
                debug!(
                    "[Filter] parse_query({:?}) -> partial prefix {:?}",
                    query, provider
                );
                return ParsedQuery {
                    prefix: Some(provider),
                    tag_filter: None,
                    query: String::new(),
                };
            }
        }

        // Dynamic plugin prefix fallback: ":word " or ":word" where word is unknown
        // Maps to Plugin(word) so user plugins with custom prefixes work
        if let Some(rest) = trimmed.strip_prefix(':') {
            if let Some(space_idx) = rest.find(' ') {
                let prefix_word = &rest[..space_idx];
                if !prefix_word.is_empty()
                    && prefix_word
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    return ParsedQuery {
                        prefix: Some(ProviderType::Plugin(prefix_word.to_string())),
                        tag_filter: None,
                        query: rest[space_idx + 1..].to_string(),
                    };
                }
            } else if !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                // Partial prefix (no space yet)
                return ParsedQuery {
                    prefix: Some(ProviderType::Plugin(rest.to_string())),
                    tag_filter: None,
                    query: String::new(),
                };
            }
        }

        let result = ParsedQuery {
            prefix: None,
            tag_filter: None,
            query: query.to_string(),
        };

        #[cfg(feature = "dev-logging")]
        debug!(
            "[Filter] parse_query({:?}) -> prefix={:?}, tag={:?}, query={:?}",
            query, result.prefix, result.tag_filter, result.query
        );

        result
    }

    /// Get enabled providers for UI display (sorted)
    pub fn enabled_providers(&self) -> Vec<ProviderType> {
        let mut providers: Vec<_> = self.enabled.iter().cloned().collect();
        providers.sort_by_key(|p| match p {
            ProviderType::Application => 0,
            ProviderType::Command => 1,
            ProviderType::Dmenu => 2,
            ProviderType::Plugin(_) => 100, // Plugin providers sort after core
        });
        providers
    }

    /// Create a filter from a list of mode name strings.
    ///
    /// Maps each string to a ProviderType: "app" -> Application, "cmd" -> Command,
    /// "dmenu" -> Dmenu, anything else -> Plugin(id). An empty list produces an
    /// all-providers filter.
    pub fn from_mode_strings(modes: &[String]) -> Self {
        if modes.is_empty() {
            return Self::all();
        }
        let enabled: HashSet<ProviderType> = modes
            .iter()
            .map(|s| Self::mode_string_to_provider_type(s))
            .collect();
        Self {
            enabled,
            active_prefix: None,
            accept_all: false,
        }
    }

    /// Create a filter that accepts all providers, including any
    /// dynamically loaded plugin.
    ///
    /// Sets `accept_all` so that `is_active`/`is_enabled` return true for
    /// every `ProviderType` without maintaining a static list of plugin IDs.
    /// Core types are still placed in `enabled` for UI purposes (tab display).
    ///
    /// The daemon uses this as the default when no modes are specified.
    pub fn all() -> Self {
        let mut enabled = HashSet::new();
        enabled.insert(ProviderType::Application);
        enabled.insert(ProviderType::Command);
        enabled.insert(ProviderType::Dmenu);
        Self {
            enabled,
            active_prefix: None,
            accept_all: true,
        }
    }

    /// Map a mode string to a ProviderType.
    ///
    /// Delegates to the existing `FromStr` impl on `ProviderType` which maps
    /// "app"/"apps"/"application" -> Application, "cmd"/"command" -> Command,
    /// "dmenu" -> Dmenu, and everything else -> Plugin(id).
    pub fn mode_string_to_provider_type(mode: &str) -> ProviderType {
        mode.parse::<ProviderType>()
            .unwrap_or_else(|_| ProviderType::Plugin(mode.to_string()))
    }

    /// Get display name for current mode
    pub fn mode_display_name(&self) -> &'static str {
        if let Some(ref prefix) = self.active_prefix {
            return match prefix {
                ProviderType::Application => "Apps",
                ProviderType::Command => "Commands",
                ProviderType::Dmenu => "dmenu",
                ProviderType::Plugin(_) => "Plugin",
            };
        }

        let enabled: Vec<_> = self.enabled_providers();
        if enabled.len() == 1 {
            match &enabled[0] {
                ProviderType::Application => "Apps",
                ProviderType::Command => "Commands",
                ProviderType::Dmenu => "dmenu",
                ProviderType::Plugin(_) => "Plugin",
            }
        } else {
            "All"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_with_prefix() {
        let result = ProviderFilter::parse_query(":app firefox");
        assert_eq!(result.prefix, Some(ProviderType::Application));
        assert_eq!(result.query, "firefox");
    }

    #[test]
    fn test_parse_query_without_prefix() {
        let result = ProviderFilter::parse_query("firefox");
        assert_eq!(result.prefix, None);
        assert_eq!(result.query, "firefox");
    }

    #[test]
    fn test_parse_query_partial_prefix() {
        let result = ProviderFilter::parse_query(":cmd");
        assert_eq!(result.prefix, Some(ProviderType::Command));
        assert_eq!(result.query, "");
    }

    #[test]
    fn test_parse_query_plugin_prefix() {
        let result = ProviderFilter::parse_query(":calc 5+3");
        assert_eq!(
            result.prefix,
            Some(ProviderType::Plugin("calc".to_string()))
        );
        assert_eq!(result.query, "5+3");
    }

    #[test]
    fn slash_alias_routes_to_filesearch_like_file_prefix() {
        // `/foo` must produce the same ParsedQuery as `:file foo` so file
        // search is reachable without typing the full prefix.
        let slash = ProviderFilter::parse_query("/foo");
        let file = ProviderFilter::parse_query(":file foo");
        assert_eq!(slash.prefix, file.prefix);
        assert_eq!(slash.query, file.query);
        assert_eq!(
            slash.prefix,
            Some(ProviderType::Plugin("filesearch".to_string()))
        );
        assert_eq!(slash.query, "foo");

        // `/ foo` (space after slash) trims to the same term.
        let spaced = ProviderFilter::parse_query("/ foo");
        assert_eq!(spaced.query, "foo");

        // Bare `/` is a partial prefix with an empty query.
        let bare = ProviderFilter::parse_query("/");
        assert_eq!(
            bare.prefix,
            Some(ProviderType::Plugin("filesearch".to_string()))
        );
        assert_eq!(bare.query, "");
    }

    #[test]
    fn parse_query_routes_unknown_prefix_to_plugin_type_id() {
        // Locks in the dynamic-prefix-fallback path (filter.rs:319-347): a
        // prefix not in the hardcoded core/plugin tables — e.g. a user
        // provider's `:smoke` — must still produce Plugin("smoke"). This is
        // what makes user-defined providers reachable by prefix.
        let result = ProviderFilter::parse_query(":smoke item");
        assert_eq!(
            result.prefix,
            Some(ProviderType::Plugin("smoke".to_string()))
        );
        assert_eq!(result.query, "item");

        let bare = ProviderFilter::parse_query(":smoke");
        assert_eq!(bare.prefix, Some(ProviderType::Plugin("smoke".to_string())));
        assert_eq!(bare.query, "");
    }

    #[test]
    fn build_prefix_param_round_trips_through_provider_type() {
        // The wire format is ProviderType's Display impl; the daemon parses
        // it back via FromStr. Verify both ends agree for a user provider.
        use std::str::FromStr;
        let plugin_smoke = ProviderType::Plugin("smoke".to_string());
        let wire = plugin_smoke.to_string();
        assert_eq!(wire, "smoke");
        let parsed = ProviderType::from_str(&wire).unwrap();
        assert_eq!(parsed, plugin_smoke);
    }

    #[test]
    fn test_toggle_ensures_one_enabled() {
        let mut filter = ProviderFilter::apps_only();
        filter.toggle(ProviderType::Application);
        // Should still have apps enabled as fallback
        assert!(filter.is_enabled(ProviderType::Application));
    }

    #[test]
    fn test_from_mode_strings_single_core() {
        let filter = ProviderFilter::from_mode_strings(&["app".to_string()]);
        assert!(filter.is_enabled(ProviderType::Application));
        assert!(!filter.is_enabled(ProviderType::Command));
    }

    #[test]
    fn test_from_mode_strings_multiple() {
        let filter = ProviderFilter::from_mode_strings(&[
            "app".to_string(),
            "cmd".to_string(),
            "calc".to_string(),
        ]);
        assert!(filter.is_enabled(ProviderType::Application));
        assert!(filter.is_enabled(ProviderType::Command));
        assert!(filter.is_enabled(ProviderType::Plugin("calc".to_string())));
        assert!(!filter.is_enabled(ProviderType::Dmenu));
    }

    #[test]
    fn test_from_mode_strings_empty_returns_all() {
        let filter = ProviderFilter::from_mode_strings(&[]);
        assert!(filter.is_enabled(ProviderType::Application));
        assert!(filter.is_enabled(ProviderType::Command));
        assert!(filter.is_enabled(ProviderType::Dmenu));
    }

    #[test]
    fn test_from_mode_strings_plugin() {
        let filter = ProviderFilter::from_mode_strings(&["emoji".to_string()]);
        assert!(filter.is_enabled(ProviderType::Plugin("emoji".to_string())));
        assert!(!filter.is_enabled(ProviderType::Application));
    }

    #[test]
    fn test_from_mode_strings_dmenu() {
        let filter = ProviderFilter::from_mode_strings(&["dmenu".to_string()]);
        assert!(filter.is_enabled(ProviderType::Dmenu));
        assert!(!filter.is_enabled(ProviderType::Application));
    }

    #[test]
    fn test_all_includes_core_types() {
        let filter = ProviderFilter::all();
        assert!(filter.is_enabled(ProviderType::Application));
        assert!(filter.is_enabled(ProviderType::Command));
        assert!(filter.is_enabled(ProviderType::Dmenu));
    }

    #[test]
    fn test_all_accepts_any_plugin() {
        let filter = ProviderFilter::all();
        // Known plugins
        assert!(filter.is_enabled(ProviderType::Plugin("calc".to_string())));
        assert!(filter.is_enabled(ProviderType::Plugin("clipboard".to_string())));
        // Arbitrary unknown plugins must also be accepted
        assert!(filter.is_enabled(ProviderType::Plugin("some-future-plugin".to_string())));
        assert!(filter.is_enabled(ProviderType::Plugin("custom-user-plugin".to_string())));
    }

    #[test]
    fn test_all_is_active_for_any_plugin() {
        let filter = ProviderFilter::all();
        assert!(filter.is_active(ProviderType::Application));
        assert!(filter.is_active(ProviderType::Plugin("unknown-plugin".to_string())));
    }

    #[test]
    fn test_all_with_prefix_narrows_scope() {
        let mut filter = ProviderFilter::all();
        filter.set_prefix(Some(ProviderType::Application));
        // Prefix narrows: only Application passes
        assert!(filter.is_active(ProviderType::Application));
        assert!(!filter.is_active(ProviderType::Command));
        assert!(!filter.is_active(ProviderType::Plugin("calc".to_string())));
    }

    #[test]
    fn test_explicit_mode_filter_rejects_unknown_plugins() {
        let filter = ProviderFilter::from_mode_strings(&["app".to_string(), "cmd".to_string()]);
        assert!(filter.is_active(ProviderType::Application));
        assert!(filter.is_active(ProviderType::Command));
        // Plugins not in the explicit list must be rejected
        assert!(!filter.is_active(ProviderType::Plugin("calc".to_string())));
        assert!(!filter.is_active(ProviderType::Plugin("unknown".to_string())));
    }

    #[test]
    fn test_mode_string_to_provider_type_core() {
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("app"),
            ProviderType::Application
        );
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("cmd"),
            ProviderType::Command
        );
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("dmenu"),
            ProviderType::Dmenu
        );
    }

    #[test]
    fn test_mode_string_to_provider_type_plugin() {
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("calc"),
            ProviderType::Plugin("calc".to_string())
        );
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("websearch"),
            ProviderType::Plugin("websearch".to_string())
        );
    }

    #[test]
    fn test_mode_string_to_provider_type_aliases() {
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("apps"),
            ProviderType::Application
        );
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("application"),
            ProviderType::Application
        );
        assert_eq!(
            ProviderFilter::mode_string_to_provider_type("command"),
            ProviderType::Command
        );
    }
}
