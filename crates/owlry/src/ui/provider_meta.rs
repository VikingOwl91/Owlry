use crate::ipc::ProviderDesc;
use crate::providers::ProviderType;

/// Display metadata for a provider.
pub struct ProviderMeta {
    pub tab_label: String,
    pub css_class: String,
    pub search_noun: String,
}

/// Resolve display metadata, checking IPC-provided metadata first, then hardcoded fallback.
pub fn resolve(provider: &ProviderType, desc: Option<&ProviderDesc>) -> ProviderMeta {
    let fallback = hardcoded(provider);
    ProviderMeta {
        tab_label: desc
            .and_then(|d| d.tab_label.clone())
            .unwrap_or(fallback.tab_label),
        css_class: fallback.css_class, // always auto-derived from type_id
        search_noun: desc
            .and_then(|d| d.search_noun.clone())
            .unwrap_or(fallback.search_noun),
    }
}

/// Hardcoded metadata for known provider types. Unknown plugins get auto-generated defaults.
fn hardcoded(provider: &ProviderType) -> ProviderMeta {
    match provider {
        ProviderType::Application => ProviderMeta {
            tab_label: "Apps".to_string(),
            css_class: "owlry-filter-app".to_string(),
            search_noun: "applications".to_string(),
        },
        ProviderType::Command => ProviderMeta {
            tab_label: "Cmds".to_string(),
            css_class: "owlry-filter-cmd".to_string(),
            search_noun: "commands".to_string(),
        },
        ProviderType::Dmenu => ProviderMeta {
            tab_label: "Dmenu".to_string(),
            css_class: "owlry-filter-dmenu".to_string(),
            search_noun: "options".to_string(),
        },
        ProviderType::Plugin(type_id) => match type_id.as_str() {
            "bookmarks" => ProviderMeta {
                tab_label: "Bookmarks".to_string(),
                css_class: "owlry-filter-bookmark".to_string(),
                search_noun: "bookmarks".to_string(),
            },
            "calc" => ProviderMeta {
                tab_label: "Calc".to_string(),
                css_class: "owlry-filter-calc".to_string(),
                search_noun: "calculator".to_string(),
            },
            "clipboard" => ProviderMeta {
                tab_label: "Clip".to_string(),
                css_class: "owlry-filter-clip".to_string(),
                search_noun: "clipboard".to_string(),
            },
            "emoji" => ProviderMeta {
                tab_label: "Emoji".to_string(),
                css_class: "owlry-filter-emoji".to_string(),
                search_noun: "emoji".to_string(),
            },
            "filesearch" => ProviderMeta {
                tab_label: "Files".to_string(),
                css_class: "owlry-filter-file".to_string(),
                search_noun: "files".to_string(),
            },
            "media" => ProviderMeta {
                tab_label: "Media".to_string(),
                css_class: "owlry-filter-media".to_string(),
                search_noun: "media".to_string(),
            },
            "pomodoro" => ProviderMeta {
                tab_label: "Pomo".to_string(),
                css_class: "owlry-filter-pomodoro".to_string(),
                search_noun: "pomodoro".to_string(),
            },
            "scripts" => ProviderMeta {
                tab_label: "Scripts".to_string(),
                css_class: "owlry-filter-script".to_string(),
                search_noun: "scripts".to_string(),
            },
            "ssh" => ProviderMeta {
                tab_label: "SSH".to_string(),
                css_class: "owlry-filter-ssh".to_string(),
                search_noun: "SSH hosts".to_string(),
            },
            "system" => ProviderMeta {
                tab_label: "System".to_string(),
                css_class: "owlry-filter-sys".to_string(),
                search_noun: "system".to_string(),
            },
            "uuctl" => ProviderMeta {
                tab_label: "uuctl".to_string(),
                css_class: "owlry-filter-uuctl".to_string(),
                search_noun: "uuctl units".to_string(),
            },
            "weather" => ProviderMeta {
                tab_label: "Weather".to_string(),
                css_class: "owlry-filter-weather".to_string(),
                search_noun: "weather".to_string(),
            },
            "websearch" => ProviderMeta {
                tab_label: "Web".to_string(),
                css_class: "owlry-filter-web".to_string(),
                search_noun: "web".to_string(),
            },
            // Unknown plugin: auto-generate from type_id
            _ => {
                let label = capitalize(type_id);
                ProviderMeta {
                    css_class: format!("owlry-filter-{}", type_id),
                    search_noun: type_id.to_string(),
                    tab_label: label,
                }
            }
        },
    }
}

/// Capitalize first character of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_plugin_returns_hardcoded() {
        let meta = resolve(&ProviderType::Plugin("calc".to_string()), None);
        assert_eq!(meta.tab_label, "Calc");
        assert_eq!(meta.css_class, "owlry-filter-calc");
        assert_eq!(meta.search_noun, "calculator");
    }

    #[test]
    fn unknown_plugin_generates_from_type_id() {
        let meta = resolve(&ProviderType::Plugin("hs".to_string()), None);
        assert_eq!(meta.tab_label, "Hs");
        assert_eq!(meta.css_class, "owlry-filter-hs");
        assert_eq!(meta.search_noun, "hs");
    }

    #[test]
    fn ipc_metadata_overrides_hardcoded() {
        let desc = ProviderDesc {
            id: "hs".to_string(),
            name: "Hypr Shutdown".to_string(),
            prefix: None,
            icon: "system-shutdown".to_string(),
            position: "normal".to_string(),
            tab_label: Some("Shutdown".to_string()),
            search_noun: Some("shutdown actions".to_string()),
        };
        let meta = resolve(&ProviderType::Plugin("hs".to_string()), Some(&desc));
        assert_eq!(meta.tab_label, "Shutdown");
        assert_eq!(meta.search_noun, "shutdown actions");
        // css_class always from hardcoded
        assert_eq!(meta.css_class, "owlry-filter-hs");
    }

    #[test]
    fn ipc_partial_override_falls_back() {
        let desc = ProviderDesc {
            id: "calc".to_string(),
            name: "Calculator".to_string(),
            prefix: None,
            icon: "calc".to_string(),
            position: "normal".to_string(),
            tab_label: Some("Calculator".to_string()),
            search_noun: None, // not provided
        };
        let meta = resolve(&ProviderType::Plugin("calc".to_string()), Some(&desc));
        assert_eq!(meta.tab_label, "Calculator"); // from IPC
        assert_eq!(meta.search_noun, "calculator"); // from hardcoded fallback
    }

    #[test]
    fn core_types_unaffected() {
        let meta = resolve(&ProviderType::Application, None);
        assert_eq!(meta.tab_label, "Apps");
        assert_eq!(meta.css_class, "owlry-filter-app");
    }
}
