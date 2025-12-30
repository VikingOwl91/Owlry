//! Web Search Plugin for Owlry
//!
//! A dynamic provider that opens web searches in the browser.
//! Supports multiple search engines.
//!
//! Examples:
//! - `? rust programming` → Search DuckDuckGo for "rust programming"
//! - `web rust docs` → Search for "rust docs"
//! - `search how to rust` → Search for "how to rust"

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};

// Plugin metadata
const PLUGIN_ID: &str = "websearch";
const PLUGIN_NAME: &str = "Web Search";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Search the web with configurable search engines";

// Provider metadata
const PROVIDER_ID: &str = "websearch";
const PROVIDER_NAME: &str = "Web Search";
const PROVIDER_PREFIX: &str = "?";
const PROVIDER_ICON: &str = "web-browser";
const PROVIDER_TYPE_ID: &str = "websearch";

/// Common search engine URL templates
/// {query} is replaced with the URL-encoded search term
const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("google", "https://www.google.com/search?q={query}"),
    ("duckduckgo", "https://duckduckgo.com/?q={query}"),
    ("bing", "https://www.bing.com/search?q={query}"),
    ("startpage", "https://www.startpage.com/search?q={query}"),
    ("searxng", "https://searx.be/search?q={query}"),
    ("brave", "https://search.brave.com/search?q={query}"),
    ("ecosia", "https://www.ecosia.org/search?q={query}"),
];

/// Default search engine if not configured
const DEFAULT_ENGINE: &str = "duckduckgo";

/// Web search provider state
struct WebSearchState {
    /// URL template with {query} placeholder
    url_template: String,
}

impl WebSearchState {
    fn new() -> Self {
        Self::with_engine(DEFAULT_ENGINE)
    }

    fn with_engine(engine_name: &str) -> Self {
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

    /// Extract the search term from the query
    fn extract_search_term(query: &str) -> Option<&str> {
        let trimmed = query.trim();

        if let Some(rest) = trimmed.strip_prefix("? ") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix("?") {
            Some(rest.trim())
        } else if trimmed.to_lowercase().starts_with("web ") {
            Some(trimmed[4..].trim())
        } else if trimmed.to_lowercase().starts_with("search ") {
            Some(trimmed[7..].trim())
        } else {
            // In filter mode, accept raw query
            Some(trimmed)
        }
    }

    /// URL-encode a search query
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

    /// Build the search URL from a query
    fn build_search_url(&self, search_term: &str) -> String {
        let encoded = Self::url_encode(search_term);
        self.url_template.replace("{query}", &encoded)
    }

    /// Evaluate a query and return a PluginItem if valid
    fn evaluate(&self, query: &str) -> Option<PluginItem> {
        let search_term = Self::extract_search_term(query)?;

        if search_term.is_empty() {
            return None;
        }

        let url = self.build_search_url(search_term);

        // Use xdg-open to open the browser
        let command = format!("xdg-open '{}'", url);

        Some(
            PluginItem::new(
                format!("websearch:{}", search_term),
                format!("Search: {}", search_term),
                command,
            )
            .with_description("Open in browser")
            .with_icon(PROVIDER_ICON)
            .with_keywords(vec!["web".to_string(), "search".to_string()]),
        )
    }
}

// ============================================================================
// Plugin Interface Implementation
// ============================================================================

extern "C" fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: RString::from(PLUGIN_ID),
        name: RString::from(PLUGIN_NAME),
        version: RString::from(PLUGIN_VERSION),
        description: RString::from(PLUGIN_DESCRIPTION),
        api_version: API_VERSION,
    }
}

extern "C" fn plugin_providers() -> RVec<ProviderInfo> {
    vec![ProviderInfo {
        id: RString::from(PROVIDER_ID),
        name: RString::from(PROVIDER_NAME),
        prefix: ROption::RSome(RString::from(PROVIDER_PREFIX)),
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Dynamic,
        type_id: RString::from(PROVIDER_TYPE_ID),
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    // TODO: Read search engine from config when plugin config is available
    let state = Box::new(WebSearchState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(_handle: ProviderHandle) -> RVec<PluginItem> {
    // Dynamic provider - refresh does nothing
    RVec::new()
}

extern "C" fn provider_query(handle: ProviderHandle, query: RStr<'_>) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<WebSearchState>
    let state = unsafe { &*(handle.ptr as *const WebSearchState) };

    let query_str = query.as_str();

    match state.evaluate(query_str) {
        Some(item) => vec![item].into(),
        None => RVec::new(),
    }
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<WebSearchState>
        unsafe {
            handle.drop_as::<WebSearchState>();
        }
    }
}

// Register the plugin vtable
owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_search_term() {
        assert_eq!(
            WebSearchState::extract_search_term("? rust programming"),
            Some("rust programming")
        );
        assert_eq!(
            WebSearchState::extract_search_term("?rust"),
            Some("rust")
        );
        assert_eq!(
            WebSearchState::extract_search_term("web rust docs"),
            Some("rust docs")
        );
        assert_eq!(
            WebSearchState::extract_search_term("search how to rust"),
            Some("how to rust")
        );
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(WebSearchState::url_encode("hello world"), "hello+world");
        assert_eq!(WebSearchState::url_encode("foo&bar"), "foo%26bar");
        assert_eq!(WebSearchState::url_encode("a=b"), "a%3Db");
        assert_eq!(WebSearchState::url_encode("test?query"), "test%3Fquery");
    }

    #[test]
    fn test_build_search_url() {
        let state = WebSearchState::with_engine("duckduckgo");
        let url = state.build_search_url("rust programming");
        assert_eq!(url, "https://duckduckgo.com/?q=rust+programming");
    }

    #[test]
    fn test_build_search_url_google() {
        let state = WebSearchState::with_engine("google");
        let url = state.build_search_url("rust");
        assert_eq!(url, "https://www.google.com/search?q=rust");
    }

    #[test]
    fn test_evaluate() {
        let state = WebSearchState::new();
        let item = state.evaluate("? rust docs").unwrap();
        assert_eq!(item.name.as_str(), "Search: rust docs");
        assert!(item.command.as_str().contains("xdg-open"));
        assert!(item.command.as_str().contains("duckduckgo"));
    }

    #[test]
    fn test_evaluate_empty() {
        let state = WebSearchState::new();
        assert!(state.evaluate("?").is_none());
        assert!(state.evaluate("? ").is_none());
    }

    #[test]
    fn test_custom_url_template() {
        let state = WebSearchState::with_engine("https://custom.search/q={query}");
        let url = state.build_search_url("test");
        assert_eq!(url, "https://custom.search/q=test");
    }

    #[test]
    fn test_fallback_to_default() {
        let state = WebSearchState::with_engine("nonexistent");
        let url = state.build_search_url("test");
        assert!(url.contains("duckduckgo")); // Falls back to default
    }
}
