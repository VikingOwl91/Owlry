use owlry::ipc::{ProviderDesc, Request, Response, ResultItem};

#[test]
fn test_query_request_roundtrip() {
    let req = Request::Query {
        text: "fire".into(),
        modes: Some(vec!["app".into(), "cmd".into()]),
        prefix: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_query_request_without_modes() {
    let req = Request::Query {
        text: "fire".into(),
        modes: None,
        prefix: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(!json.contains("modes"));
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_launch_request_roundtrip() {
    let req = Request::Launch {
        item_id: "firefox.desktop".into(),
        provider: "app".into(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_results_response_roundtrip() {
    let resp = Response::Results {
        items: vec![ResultItem {
            id: "firefox.desktop".into(),
            title: "Firefox".into(),
            description: "Web Browser".into(),
            icon: "firefox".into(),
            provider: "app".into(),
            score: 95,
            command: Some("firefox".into()),
            terminal: false,
            activation: owlry::providers::ItemActivation::Launch,
            tags: vec![],
            source: "core".into(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, parsed);
}

#[test]
fn test_providers_response() {
    let resp = Response::Providers {
        list: vec![ProviderDesc {
            id: "app".into(),
            name: "Applications".into(),
            prefix: Some(":app".into()),
            icon: "application-x-executable".into(),
            position: "normal".into(),
            tab_label: None,
            search_noun: None,
            kind: "static".into(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, parsed);
}

#[test]
fn test_error_response() {
    let resp = Response::Error {
        message: "plugin not found".into(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, parsed);
}

#[test]
fn test_toggle_request() {
    let req = Request::Toggle;
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_submenu_request() {
    let req = Request::Submenu {
        plugin_id: "systemd".into(),
        data: "docker.service".into(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_refresh_request() {
    let req = Request::Refresh {
        provider: Some("clipboard".into()),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_refresh_all_request_omits_provider() {
    let req = Request::Refresh { provider: None };
    let json = serde_json::to_string(&req).unwrap();
    assert!(!json.contains("provider"));
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_plugin_action_request() {
    let req = Request::PluginAction {
        command: "POMODORO:start".into(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(req, parsed);
}

#[test]
fn test_terminal_field_defaults_false() {
    // terminal field should default to false when missing from JSON
    let json =
        r#"{"id":"test","title":"Test","description":"","icon":"","provider":"cmd","score":0}"#;
    let item: ResultItem = serde_json::from_str(json).unwrap();
    assert!(!item.terminal);
}

#[test]
fn test_plugin_list_request_is_rejected() {
    // v2 dropped Request::PluginList. The daemon must reject incoming
    // `{"type":"plugin_list"}` requests so old clients fail loudly instead
    // of silently appearing to work.
    let result: Result<Request, _> = serde_json::from_str(r#"{"type":"plugin_list"}"#);
    assert!(result.is_err(), "plugin_list request must not deserialize");
}

#[test]
fn test_plugin_list_response_is_rejected() {
    // Same on the response side — old daemons replying with
    // `{"type":"plugin_list", ...}` must fail to parse so clients can't
    // accidentally treat them as valid.
    let result: Result<Response, _> =
        serde_json::from_str(r#"{"type":"plugin_list","entries":[]}"#);
    assert!(result.is_err(), "plugin_list response must not deserialize");
}

#[test]
fn test_terminal_field_roundtrip() {
    let item = ResultItem {
        id: "htop".into(),
        title: "htop".into(),
        description: "Process viewer".into(),
        icon: "htop".into(),
        provider: "cmd".into(),
        score: 50,
        command: Some("htop".into()),
        terminal: true,
        activation: owlry::providers::ItemActivation::Launch,
        tags: vec![],
        source: "cmd".into(),
    };
    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"terminal\":true"));
    let parsed: ResultItem = serde_json::from_str(&json).unwrap();
    assert!(parsed.terminal);
}
