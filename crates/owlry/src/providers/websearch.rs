use super::{DynamicProvider, ItemSource, LaunchItem, ProviderType};

/// Built-in web search provider. Opens a search URL in the user's browser via
/// `xdg-open`.
///
/// Triggered by:
/// - `? query` / `?query` (explicit prefix)
/// - `web query` / `search query` (word prefixes)
///
/// The CLI prefix routing for `:web` and `?` is handled by core; this provider
/// only needs to return matching items when `query()` is called.
pub(crate) struct WebSearchProvider {
    /// URL template containing a `{query}` placeholder.
    url_template: String,
}

/// Common search engine URL templates. `{query}` is replaced with the
/// URL-encoded search term.
const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("google", "https://www.google.com/search?q={query}"),
    ("duckduckgo", "https://duckduckgo.com/?q={query}"),
    ("bing", "https://www.bing.com/search?q={query}"),
    ("startpage", "https://www.startpage.com/search?q={query}"),
    ("searxng", "https://searx.be/search?q={query}"),
    ("brave", "https://search.brave.com/search?q={query}"),
    ("ecosia", "https://www.ecosia.org/search?q={query}"),
];

/// Default search engine when no configuration is provided.
const DEFAULT_ENGINE: &str = "duckduckgo";

const PROVIDER_ICON: &str = "web-browser";

impl Default for WebSearchProvider {
    fn default() -> Self {
        // TODO(v2.x): plumb search_engine via constructor argument from Lua
        // config. The C-ABI host config lookup was removed; for now we
        // hardcode the default engine.
        Self::with_engine(DEFAULT_ENGINE)
    }
}

impl WebSearchProvider {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_engine(engine_name: &str) -> Self {
        let lower = engine_name.to_lowercase();
        let url_template = SEARCH_ENGINES
            .iter()
            .find(|(name, _)| *name == lower)
            .map(|(_, url)| (*url).to_string())
            .unwrap_or_else(|| {
                // Not a known engine; treat as custom URL template if it
                // contains a {query} placeholder, else fall back to default.
                if engine_name.contains("{query}") {
                    engine_name.to_string()
                } else {
                    SEARCH_ENGINES
                        .iter()
                        .find(|(name, _)| *name == DEFAULT_ENGINE)
                        .map(|(_, url)| (*url).to_string())
                        .expect("default engine must exist in SEARCH_ENGINES")
                }
            });

        Self { url_template }
    }

    /// Extract the search term from a raw query string.
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();

        if let Some(rest) = trimmed.strip_prefix("? ") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix('?') {
            Some(rest.trim())
        } else if trimmed.to_lowercase().starts_with("web ") {
            Some(trimmed[4..].trim())
        } else if trimmed.to_lowercase().starts_with("search ") {
            Some(trimmed[7..].trim())
        } else {
            // Filter mode: accept the raw query.
            Some(trimmed)
        }
    }

    /// URL-encode a search query using a small percent-encoding scheme suitable
    /// for query parameters (spaces become `+`).
    fn url_encode(query: &str) -> String {
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

    /// Build the final search URL for a given search term.
    fn build_search_url(&self, search_term: &str) -> String {
        let encoded = Self::url_encode(search_term);
        self.url_template.replace("{query}", &encoded)
    }

    /// Evaluate a query and produce a single `LaunchItem` if the query yields a
    /// non-empty search term.
    fn evaluate(&self, query: &str) -> Option<LaunchItem> {
        let search_term = Self::extract_search_term(query)?;
        if search_term.is_empty() {
            return None;
        }

        let url = self.build_search_url(search_term);
        let command = format!("xdg-open '{}'", url.replace('\'', "'\\''"));

        Some(LaunchItem {
            id: format!("websearch:{}", search_term),
            name: format!("Search: {}", search_term),
            description: Some("Open in browser".to_string()),
            icon: Some(PROVIDER_ICON.to_string()),
            provider: ProviderType::Plugin("websearch".into()),
            command,
            terminal: false,
            activation: super::ItemActivation::Launch,
            tags: vec!["web".into(), "search".into()],
            source: ItemSource::Core,
        })
    }
}

impl DynamicProvider for WebSearchProvider {
    fn name(&self) -> &str {
        "Web Search"
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Plugin("websearch".into())
    }

    fn priority(&self) -> u32 {
        9_000
    }

    fn prefix(&self) -> Option<&str> {
        Some(":web")
    }

    fn icon(&self) -> &str {
        "applications-internet"
    }

    fn tab_label(&self) -> Option<&str> {
        Some("Web")
    }

    fn search_noun(&self) -> Option<&str> {
        Some("the web")
    }

    fn query(&self, query: &str) -> Vec<LaunchItem> {
        match self.evaluate(query) {
            Some(item) => vec![item],
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            WebSearchProvider::extract_search_term("search how to rust"),
            Some("how to rust")
        );
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(WebSearchProvider::url_encode("hello world"), "hello+world");
        assert_eq!(WebSearchProvider::url_encode("foo&bar"), "foo%26bar");
        assert_eq!(WebSearchProvider::url_encode("a=b"), "a%3Db");
        assert_eq!(WebSearchProvider::url_encode("test?query"), "test%3Fquery");
    }

    #[test]
    fn test_build_search_url() {
        let provider = WebSearchProvider::with_engine("duckduckgo");
        let url = provider.build_search_url("rust programming");
        assert_eq!(url, "https://duckduckgo.com/?q=rust+programming");
    }

    #[test]
    fn test_build_search_url_google() {
        let provider = WebSearchProvider::with_engine("google");
        let url = provider.build_search_url("rust");
        assert_eq!(url, "https://www.google.com/search?q=rust");
    }

    #[test]
    fn test_evaluate() {
        let provider = WebSearchProvider::new();
        let item = provider.evaluate("? rust docs").unwrap();
        assert_eq!(item.name, "Search: rust docs");
        assert!(item.command.contains("xdg-open"));
        assert!(item.command.contains("duckduckgo"));
    }

    #[test]
    fn test_evaluate_empty() {
        let provider = WebSearchProvider::new();
        assert!(provider.evaluate("?").is_none());
        assert!(provider.evaluate("? ").is_none());
    }

    #[test]
    fn test_custom_url_template() {
        let provider = WebSearchProvider::with_engine("https://custom.search/q={query}");
        let url = provider.build_search_url("test");
        assert_eq!(url, "https://custom.search/q=test");
    }

    #[test]
    fn test_fallback_to_default() {
        let provider = WebSearchProvider::with_engine("nonexistent");
        let url = provider.build_search_url("test");
        assert!(url.contains("duckduckgo"));
    }

    #[test]
    fn provider_type_is_websearch_plugin() {
        assert_eq!(
            WebSearchProvider::default().provider_type(),
            ProviderType::Plugin("websearch".into())
        );
    }
}
