use crate::providers::{LaunchItem, ProviderType};

/// Common search engine URL templates
/// {query} is replaced with the URL-encoded search term
pub const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("google", "https://www.google.com/search?q={query}"),
    ("duckduckgo", "https://duckduckgo.com/?q={query}"),
    ("bing", "https://www.bing.com/search?q={query}"),
    ("startpage", "https://www.startpage.com/search?q={query}"),
    ("searxng", "https://searx.be/search?q={query}"),
    ("brave", "https://search.brave.com/search?q={query}"),
    ("ecosia", "https://www.ecosia.org/search?q={query}"),
];

/// Default search engine if not configured
pub const DEFAULT_ENGINE: &str = "duckduckgo";

/// Web search provider - opens browser with search query
pub struct WebSearchProvider {
    /// URL template with {query} placeholder
    url_template: String,
}

impl WebSearchProvider {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_engine(DEFAULT_ENGINE)
    }

    /// Create provider with specific search engine
    pub fn with_engine(engine_name: &str) -> Self {
        let url_template = SEARCH_ENGINES
            .iter()
            .find(|(name, _)| *name == engine_name.to_lowercase())
            .map(|(_, url)| url.to_string())
            .unwrap_or_else(|| {
                // If not a known engine, treat it as a custom URL template
                if engine_name.contains("{query}") {
                    engine_name.to_string()
                } else {
                    // Fall back to default
                    SEARCH_ENGINES
                        .iter()
                        .find(|(name, _)| *name == DEFAULT_ENGINE)
                        .map(|(_, url)| url.to_string())
                        .unwrap()
                }
            });

        Self { url_template }
    }

    /// Check if query is a web search query
    /// Triggers on: `? query`, `web query`, `search query`
    pub fn is_websearch_query(query: &str) -> bool {
        let trimmed = query.trim();
        trimmed.starts_with("? ")
            || trimmed.starts_with("?")
            || trimmed.to_lowercase().starts_with("web ")
            || trimmed.to_lowercase().starts_with("search ")
    }

    /// Extract the search term from the query
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();

        if let Some(rest) = trimmed.strip_prefix("? ") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix("?") {
            Some(rest.trim())
        } else if trimmed.to_lowercase().starts_with("web ") {
            // Need to get the original casing
            Some(trimmed[4..].trim())
        } else if trimmed.to_lowercase().starts_with("search ") {
            Some(trimmed[7..].trim())
        } else {
            None
        }
    }

    /// URL-encode a search query
    fn url_encode(query: &str) -> String {
        // TODO: This is where you can implement the URL encoding logic!
        // Consider: Should we use a crate like `urlencoding` or implement manually?
        // Manual encoding needs to handle: spaces, &, =, ?, #, etc.
        query
            .chars()
            .map(|c| match c {
                ' ' => "+".to_string(),
                '&' => "%26".to_string(),
                '=' => "%3D".to_string(),
                '?' => "%3F".to_string(),
                '#' => "%23".to_string(),
                '+' => "%2B".to_string(),
                '%' => "%25".to_string(),
                c if c.is_ascii_alphanumeric() || "-_.~".contains(c) => c.to_string(),
                c => format!("%{:02X}", c as u32),
            })
            .collect()
    }

    /// Build the search URL from a query
    fn build_search_url(&self, search_term: &str) -> String {
        let encoded = Self::url_encode(search_term);
        self.url_template.replace("{query}", &encoded)
    }

    /// Evaluate a web search query and return a LaunchItem if valid
    pub fn evaluate(&self, query: &str) -> Option<LaunchItem> {
        let search_term = Self::extract_search_term(query)?;

        if search_term.is_empty() {
            return None;
        }

        self.evaluate_raw(search_term)
    }

    /// Evaluate a raw search term (for :web filter mode)
    pub fn evaluate_raw(&self, search_term: &str) -> Option<LaunchItem> {
        let trimmed = search_term.trim();
        if trimmed.is_empty() {
            return None;
        }

        let url = self.build_search_url(trimmed);

        // Use xdg-open to open the browser
        let command = format!("xdg-open '{}'", url);

        Some(LaunchItem {
            id: format!("websearch:{}", trimmed),
            name: format!("Search: {}", trimmed),
            description: Some("Open in browser".to_string()),
            icon: Some("web-browser".to_string()),
            provider: ProviderType::WebSearch,
            command,
            terminal: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_websearch_query() {
        assert!(WebSearchProvider::is_websearch_query("? rust programming"));
        assert!(WebSearchProvider::is_websearch_query("?rust"));
        assert!(WebSearchProvider::is_websearch_query("web rust"));
        assert!(WebSearchProvider::is_websearch_query("search rust"));
        assert!(!WebSearchProvider::is_websearch_query("rust"));
        assert!(!WebSearchProvider::is_websearch_query("= 5+3"));
    }

    #[test]
    fn test_extract_search_term() {
        assert_eq!(
            WebSearchProvider::extract_search_term("? rust programming"),
            Some("rust programming")
        );
        assert_eq!(
            WebSearchProvider::extract_search_term("?rust"),
            Some("rust")
        );
        assert_eq!(
            WebSearchProvider::extract_search_term("web rust docs"),
            Some("rust docs")
        );
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(WebSearchProvider::url_encode("hello world"), "hello+world");
        assert_eq!(WebSearchProvider::url_encode("foo&bar"), "foo%26bar");
        assert_eq!(WebSearchProvider::url_encode("a=b"), "a%3Db");
    }

    #[test]
    fn test_build_search_url() {
        let provider = WebSearchProvider::with_engine("duckduckgo");
        let url = provider.build_search_url("rust programming");
        assert_eq!(url, "https://duckduckgo.com/?q=rust+programming");
    }

    #[test]
    fn test_evaluate() {
        let provider = WebSearchProvider::new();
        let item = provider.evaluate("? rust docs").unwrap();
        assert_eq!(item.name, "Search: rust docs");
        assert!(item.command.contains("xdg-open"));
        assert!(item.command.contains("duckduckgo"));
    }
}
