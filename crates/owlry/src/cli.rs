//! Command-line interface for owlry launcher

use clap::Parser;

use crate::providers::ProviderType;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "owlry",
    about = "An owl-themed application launcher for Wayland",
    long_about = "An owl-themed application launcher for Wayland, built with GTK4 and Layer Shell.\n\n\
                  Owlry provides fuzzy search across applications, commands, and built-in providers.",
    version,
    after_help = "\
EXAMPLES:
    owlry                           Launch UI, auto mode
    owlry -m auto                   Launch UI, auto mode (explicit)
    owlry -m app                    Applications only
    owlry -m cmd                    PATH commands only
    owlry -m dmenu                  dmenu-compatible mode (reads from stdin)
    owlry --profile dev             Use a named profile from config
    owlry -d                        Run the daemon

DMENU MODE:
    Pipe input to owlry for interactive selection:

    echo -e \"Option A\\nOption B\" | owlry -m dmenu
    ls | owlry -m dmenu -p \"checkout:\"
    git branch | owlry -m dmenu --prompt \"checkout:\"

PROFILES:
    Define profiles in ~/.config/owlry/config.toml:

    [profiles.dev]
    modes = [\"app\", \"cmd\", \"ssh\"]

    Then launch with: owlry --profile dev

SEARCH PREFIXES:
    :app firefox                    Search applications
    :cmd git                        Search PATH commands
    :calc / =                       Calculator (e.g. = 2+2)
    :web / ?                        Web search
    :file / /                       File search

For configuration, see ~/.config/owlry/config.toml"
)]
pub struct CliArgs {
    /// Run the daemon (alias for `owlry daemon`, when subcommands land)
    #[arg(long, short = 'd', conflicts_with_all = ["mode", "profile", "prompt"])]
    pub daemon: bool,

    /// Start in single-provider mode
    ///
    /// Core modes: app, cmd, dmenu, auto
    /// Built-in modes: calc, conv, power
    /// Optional modes (cargo features): bm, clip, emoji, ssh, systemd/uuctl, web, file
    #[arg(long, short = 'm', value_parser = parse_provider, value_name = "MODE")]
    pub mode: Option<ProviderType>,

    /// Use a named profile from config (defines which modes to enable)
    ///
    /// Profiles are defined in config.toml under [profiles.<name>].
    /// Example: --profile dev (loads modes from [profiles.dev])
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Custom prompt text for the search input
    ///
    /// Useful in dmenu mode to indicate what the user is selecting.
    /// Example: -p "Select file:" or --prompt "Select file:"
    #[arg(long, short = 'p', value_name = "TEXT")]
    pub prompt: Option<String>,
}

fn parse_provider(s: &str) -> Result<ProviderType, String> {
    s.parse()
}

impl CliArgs {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_args_yields_ui_launch_defaults() {
        let args = CliArgs::try_parse_from(["owlry"]).unwrap();
        assert!(!args.daemon);
        assert!(args.mode.is_none());
        assert!(args.profile.is_none());
        assert!(args.prompt.is_none());
    }

    #[test]
    fn dash_d_enables_daemon_mode() {
        let args = CliArgs::try_parse_from(["owlry", "-d"]).unwrap();
        assert!(args.daemon);
    }

    #[test]
    fn long_daemon_flag_enables_daemon_mode() {
        let args = CliArgs::try_parse_from(["owlry", "--daemon"]).unwrap();
        assert!(args.daemon);
    }

    #[test]
    fn daemon_flag_conflicts_with_ui_flags() {
        // -d must reject UI-only flags so a single invocation can't try to
        // both run the daemon and launch a UI in single-mode.
        assert!(CliArgs::try_parse_from(["owlry", "-d", "-m", "app"]).is_err());
        assert!(CliArgs::try_parse_from(["owlry", "-d", "--profile", "dev"]).is_err());
        assert!(CliArgs::try_parse_from(["owlry", "-d", "-p", "prompt"]).is_err());
    }

    #[test]
    fn mode_flag_parses_known_providers() {
        let args = CliArgs::try_parse_from(["owlry", "-m", "app"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Application));

        let args = CliArgs::try_parse_from(["owlry", "-m", "cmd"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Command));

        let args = CliArgs::try_parse_from(["owlry", "-m", "dmenu"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Dmenu));

        // Unknown modes become Plugin(s) so user-defined providers (Phase 3+)
        // and built-in provider IDs (uuctl, bookmarks, etc.) work uniformly.
        let args = CliArgs::try_parse_from(["owlry", "-m", "uuctl"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Plugin("uuctl".into())));
    }

    #[test]
    fn plugin_subcommand_is_no_longer_recognised() {
        // The entire `plugin` subcommand tree was dropped in v2.
        assert!(CliArgs::try_parse_from(["owlry", "plugin", "list"]).is_err());
        assert!(CliArgs::try_parse_from(["owlry", "plugin", "install", "x"]).is_err());
    }
}
