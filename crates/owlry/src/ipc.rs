use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Query {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        modes: Option<Vec<String>>,
        /// Active prefix narrowing — the daemon restricts results to this
        /// single provider. Carries the [`ProviderType`] as a string
        /// (e.g. `"app"`, `"cmd"`, `"smoke"` for a user-defined provider
        /// with id `"smoke"`). The UI populates this from
        /// `ProviderFilter::parse_query`'s `prefix` field after stripping
        /// the `:foo ` from the query text. Older clients omit the field;
        /// the daemon treats `None` as "no narrowing".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Results { items: Vec<ResultItem> },
    Providers { list: Vec<ProviderDesc> },
    SubmenuItems { items: Vec<ResultItem> },
    Ack,
    Error { message: String },
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
    /// Item trust level: "core" or "script_plugin".
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_request_without_prefix_serializes_without_field() {
        // Pre-3.4.5 clients didn't have the field; new schema must omit it
        // from JSON when None so on-wire payloads stay identical.
        let req = Request::Query {
            text: "firefox".into(),
            modes: None,
            prefix: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("prefix"),
            "prefix must be omitted; got: {}",
            json
        );
        assert!(json.contains(r#""type":"query""#));
        assert!(json.contains(r#""text":"firefox""#));
    }

    #[test]
    fn query_request_with_prefix_serializes_and_roundtrips() {
        let req = Request::Query {
            text: "foo".into(),
            modes: None,
            prefix: Some("smoke".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""prefix":"smoke""#));
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn legacy_clients_without_prefix_field_still_deserialize() {
        // A 2.0.x client sends Query without the new prefix field. The 2.1
        // daemon must still accept it (default = None).
        let json = r#"{"type":"query","text":"foo"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        match req {
            Request::Query {
                text,
                modes,
                prefix,
            } => {
                assert_eq!(text, "foo");
                assert!(modes.is_none());
                assert!(prefix.is_none());
            }
            other => panic!("expected Query, got {:?}", other),
        }
    }
}
