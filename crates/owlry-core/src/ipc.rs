use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Query {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        modes: Option<Vec<String>>,
    },
    Launch {
        item_id: String,
        provider: String,
    },
    Providers,
    Refresh {
        provider: String,
    },
    Toggle,
    Submenu {
        plugin_id: String,
        data: String,
    },
    PluginAction {
        command: String,
    },
    /// Query the daemon's plugin registry (native plugins + suppressed entries).
    PluginList,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Results { items: Vec<ResultItem> },
    Providers { list: Vec<ProviderDesc> },
    SubmenuItems { items: Vec<ResultItem> },
    PluginList { entries: Vec<PluginEntry> },
    Ack,
    Error { message: String },
}

/// Registry entry for a loaded or suppressed plugin (native plugins only).
/// Script plugins are tracked separately via filesystem discovery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Plugin runtime type: "native", "builtin"
    pub runtime: String,
    /// Load status: "active" or "suppressed"
    pub status: String,
    /// Human-readable detail for non-active status (e.g. suppression reason)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_detail: String,
    /// Provider type IDs registered by this plugin
    #[serde(default)]
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub provider: String,
    pub score: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Item trust level: "core", "native_plugin", or "script_plugin".
    /// Defaults to "core" when absent (backwards-compatible with old daemons).
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "core".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDesc {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub icon: String,
    pub position: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_noun: Option<String>,
}
