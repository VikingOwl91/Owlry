use owlry_core::providers::ProviderType;

/// Display metadata for a provider.
pub struct ProviderMeta {
    pub tab_label: &'static str,
    pub css_class: &'static str,
    pub search_noun: &'static str,
}

/// Return display metadata for a provider type.
pub fn meta_for(provider: &ProviderType) -> ProviderMeta {
    match provider {
        ProviderType::Application => ProviderMeta {
            tab_label: "Apps",
            css_class: "owlry-filter-app",
            search_noun: "applications",
        },
        ProviderType::Command => ProviderMeta {
            tab_label: "Cmds",
            css_class: "owlry-filter-cmd",
            search_noun: "commands",
        },
        ProviderType::Dmenu => ProviderMeta {
            tab_label: "Dmenu",
            css_class: "owlry-filter-dmenu",
            search_noun: "options",
        },
        ProviderType::Plugin(type_id) => match type_id.as_str() {
            "bookmarks" => ProviderMeta {
                tab_label: "Bookmarks",
                css_class: "owlry-filter-bookmark",
                search_noun: "bookmarks",
            },
            "calc" => ProviderMeta {
                tab_label: "Calc",
                css_class: "owlry-filter-calc",
                search_noun: "calculator",
            },
            "clipboard" => ProviderMeta {
                tab_label: "Clip",
                css_class: "owlry-filter-clip",
                search_noun: "clipboard",
            },
            "emoji" => ProviderMeta {
                tab_label: "Emoji",
                css_class: "owlry-filter-emoji",
                search_noun: "emoji",
            },
            "filesearch" => ProviderMeta {
                tab_label: "Files",
                css_class: "owlry-filter-file",
                search_noun: "files",
            },
            "media" => ProviderMeta {
                tab_label: "Media",
                css_class: "owlry-filter-media",
                search_noun: "media",
            },
            "pomodoro" => ProviderMeta {
                tab_label: "Pomo",
                css_class: "owlry-filter-pomodoro",
                search_noun: "pomodoro",
            },
            "scripts" => ProviderMeta {
                tab_label: "Scripts",
                css_class: "owlry-filter-script",
                search_noun: "scripts",
            },
            "ssh" => ProviderMeta {
                tab_label: "SSH",
                css_class: "owlry-filter-ssh",
                search_noun: "SSH hosts",
            },
            "system" => ProviderMeta {
                tab_label: "System",
                css_class: "owlry-filter-sys",
                search_noun: "system",
            },
            "uuctl" => ProviderMeta {
                tab_label: "uuctl",
                css_class: "owlry-filter-uuctl",
                search_noun: "uuctl units",
            },
            "weather" => ProviderMeta {
                tab_label: "Weather",
                css_class: "owlry-filter-weather",
                search_noun: "weather",
            },
            "websearch" => ProviderMeta {
                tab_label: "Web",
                css_class: "owlry-filter-web",
                search_noun: "web",
            },
            _ => ProviderMeta {
                tab_label: "Plugin",
                css_class: "owlry-filter-plugin",
                search_noun: "plugins",
            },
        },
    }
}
