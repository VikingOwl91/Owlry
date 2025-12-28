use std::collections::HashSet;

use crate::config::ProvidersConfig;
use crate::providers::ProviderType;

/// Tracks which providers are enabled and handles prefix-based filtering
#[derive(Debug, Clone)]
pub struct ProviderFilter {
    enabled: HashSet<ProviderType>,
    active_prefix: Option<ProviderType>,
}

/// Result of parsing a query for prefix syntax
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub prefix: Option<ProviderType>,
    pub query: String,
}

impl ProviderFilter {
    /// Create filter from CLI args and config
    pub fn new(
        cli_mode: Option<ProviderType>,
        cli_providers: Option<Vec<ProviderType>>,
        config_providers: &ProvidersConfig,
    ) -> Self {
        let enabled = if let Some(mode) = cli_mode {
            // --mode overrides everything: single provider
            HashSet::from([mode])
        } else if let Some(providers) = cli_providers {
            // --providers overrides config
            providers.into_iter().collect()
        } else {
            // Use config file settings, default to apps only
            let mut set = HashSet::new();
            if config_providers.applications {
                set.insert(ProviderType::Application);
            }
            if config_providers.commands {
                set.insert(ProviderType::Command);
            }
            if config_providers.uuctl {
                set.insert(ProviderType::Uuctl);
            }
            // Default to apps if nothing enabled
            if set.is_empty() {
                set.insert(ProviderType::Application);
            }
            set
        };

        Self {
            enabled,
            active_prefix: None,
        }
    }

    /// Default filter: apps only
    #[allow(dead_code)]
    pub fn apps_only() -> Self {
        Self {
            enabled: HashSet::from([ProviderType::Application]),
            active_prefix: None,
        }
    }

    /// Toggle a provider on/off
    pub fn toggle(&mut self, provider: ProviderType) {
        if self.enabled.contains(&provider) {
            self.enabled.remove(&provider);
            // Ensure at least one provider is always enabled
            if self.enabled.is_empty() {
                self.enabled.insert(ProviderType::Application);
            }
        } else {
            self.enabled.insert(provider);
        }
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
    }

    /// Set prefix mode (from :app, :cmd, etc.)
    pub fn set_prefix(&mut self, prefix: Option<ProviderType>) {
        self.active_prefix = prefix;
    }

    /// Check if a provider should be searched
    pub fn is_active(&self, provider: ProviderType) -> bool {
        if let Some(prefix) = self.active_prefix {
            provider == prefix
        } else {
            self.enabled.contains(&provider)
        }
    }

    /// Check if provider is in enabled set (ignoring prefix)
    pub fn is_enabled(&self, provider: ProviderType) -> bool {
        self.enabled.contains(&provider)
    }

    /// Get current active prefix if any
    #[allow(dead_code)]
    pub fn active_prefix(&self) -> Option<ProviderType> {
        self.active_prefix
    }

    /// Parse query for prefix syntax
    pub fn parse_query(query: &str) -> ParsedQuery {
        let trimmed = query.trim_start();

        // Check for prefix patterns (with trailing space)
        let prefixes = [
            (":app ", ProviderType::Application),
            (":apps ", ProviderType::Application),
            (":bm ", ProviderType::Bookmarks),
            (":bookmark ", ProviderType::Bookmarks),
            (":bookmarks ", ProviderType::Bookmarks),
            (":calc ", ProviderType::Calculator),
            (":calculator ", ProviderType::Calculator),
            (":clip ", ProviderType::Clipboard),
            (":clipboard ", ProviderType::Clipboard),
            (":cmd ", ProviderType::Command),
            (":command ", ProviderType::Command),
            (":emoji ", ProviderType::Emoji),
            (":emojis ", ProviderType::Emoji),
            (":file ", ProviderType::Files),
            (":files ", ProviderType::Files),
            (":find ", ProviderType::Files),
            (":script ", ProviderType::Scripts),
            (":scripts ", ProviderType::Scripts),
            (":ssh ", ProviderType::Ssh),
            (":sys ", ProviderType::System),
            (":system ", ProviderType::System),
            (":power ", ProviderType::System),
            (":uuctl ", ProviderType::Uuctl),
            (":web ", ProviderType::WebSearch),
            (":search ", ProviderType::WebSearch),
        ];

        for (prefix_str, provider) in prefixes {
            if let Some(rest) = trimmed.strip_prefix(prefix_str) {
                return ParsedQuery {
                    prefix: Some(provider),
                    query: rest.to_string(),
                };
            }
        }

        // Handle prefix without trailing space (still typing)
        let partial_prefixes = [
            (":app", ProviderType::Application),
            (":apps", ProviderType::Application),
            (":bm", ProviderType::Bookmarks),
            (":bookmark", ProviderType::Bookmarks),
            (":bookmarks", ProviderType::Bookmarks),
            (":calc", ProviderType::Calculator),
            (":calculator", ProviderType::Calculator),
            (":clip", ProviderType::Clipboard),
            (":clipboard", ProviderType::Clipboard),
            (":cmd", ProviderType::Command),
            (":command", ProviderType::Command),
            (":emoji", ProviderType::Emoji),
            (":emojis", ProviderType::Emoji),
            (":file", ProviderType::Files),
            (":files", ProviderType::Files),
            (":find", ProviderType::Files),
            (":script", ProviderType::Scripts),
            (":scripts", ProviderType::Scripts),
            (":ssh", ProviderType::Ssh),
            (":sys", ProviderType::System),
            (":system", ProviderType::System),
            (":power", ProviderType::System),
            (":uuctl", ProviderType::Uuctl),
            (":web", ProviderType::WebSearch),
            (":search", ProviderType::WebSearch),
        ];

        for (prefix_str, provider) in partial_prefixes {
            if trimmed == prefix_str {
                return ParsedQuery {
                    prefix: Some(provider),
                    query: String::new(),
                };
            }
        }

        ParsedQuery {
            prefix: None,
            query: query.to_string(),
        }
    }

    /// Get enabled providers for UI display (sorted)
    pub fn enabled_providers(&self) -> Vec<ProviderType> {
        let mut providers: Vec<_> = self.enabled.iter().copied().collect();
        providers.sort_by_key(|p| match p {
            ProviderType::Application => 0,
            ProviderType::Bookmarks => 1,
            ProviderType::Calculator => 2,
            ProviderType::Clipboard => 3,
            ProviderType::Command => 4,
            ProviderType::Dmenu => 5,
            ProviderType::Emoji => 6,
            ProviderType::Files => 7,
            ProviderType::Scripts => 8,
            ProviderType::Ssh => 9,
            ProviderType::System => 10,
            ProviderType::Uuctl => 11,
            ProviderType::WebSearch => 12,
        });
        providers
    }

    /// Get display name for current mode
    pub fn mode_display_name(&self) -> &'static str {
        if let Some(prefix) = self.active_prefix {
            return match prefix {
                ProviderType::Application => "Apps",
                ProviderType::Bookmarks => "Bookmarks",
                ProviderType::Calculator => "Calc",
                ProviderType::Clipboard => "Clipboard",
                ProviderType::Command => "Commands",
                ProviderType::Dmenu => "dmenu",
                ProviderType::Emoji => "Emoji",
                ProviderType::Files => "Files",
                ProviderType::Scripts => "Scripts",
                ProviderType::Ssh => "SSH",
                ProviderType::System => "System",
                ProviderType::Uuctl => "uuctl",
                ProviderType::WebSearch => "Web",
            };
        }

        let enabled: Vec<_> = self.enabled_providers();
        if enabled.len() == 1 {
            match enabled[0] {
                ProviderType::Application => "Apps",
                ProviderType::Bookmarks => "Bookmarks",
                ProviderType::Calculator => "Calc",
                ProviderType::Clipboard => "Clipboard",
                ProviderType::Command => "Commands",
                ProviderType::Dmenu => "dmenu",
                ProviderType::Emoji => "Emoji",
                ProviderType::Files => "Files",
                ProviderType::Scripts => "Scripts",
                ProviderType::Ssh => "SSH",
                ProviderType::System => "System",
                ProviderType::Uuctl => "uuctl",
                ProviderType::WebSearch => "Web",
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
    fn test_toggle_ensures_one_enabled() {
        let mut filter = ProviderFilter::apps_only();
        filter.toggle(ProviderType::Application);
        // Should still have apps enabled as fallback
        assert!(filter.is_enabled(ProviderType::Application));
    }
}
