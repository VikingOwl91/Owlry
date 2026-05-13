//! Command-line interface for owlry launcher.

use clap::{Parser, Subcommand};

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
    owlry --profile dev             Use a named profile from config

    owlry daemon                    Run the daemon (alias: owlry -d)
    owlry dmenu [-p <prompt>]       dmenu-compatible mode (reads from stdin)
    owlry doctor                    Diagnostics: providers, config, socket
    owlry providers [<id>]          List providers (or show one)
    owlry config validate           Check config for errors
    owlry config show               Print resolved effective config
    owlry migrate-config            TOML → init.lua (Phase 3+; stub for now)

SEARCH PREFIXES (inside the UI):
    :app firefox                    Search applications
    :cmd git                        Search PATH commands
    :calc / =                       Calculator (e.g. = 2+2)
    :web / ?                        Web search
    :file / /                       File search

For configuration, see ~/.config/owlry/config.toml"
)]
pub struct CliArgs {
    /// Run the daemon (alias for `owlry daemon`).
    #[arg(long, short = 'd', conflicts_with_all = ["mode", "profile", "prompt"])]
    pub daemon: bool,

    /// Start the UI in a single-provider mode.
    ///
    /// Core modes: app, cmd, dmenu, auto
    /// Built-in modes: calc, conv, power
    /// Optional modes (cargo features): bm, clip, emoji, ssh, systemd/uuctl, web, file
    #[arg(long, short = 'm', value_parser = parse_provider, value_name = "MODE")]
    pub mode: Option<ProviderType>,

    /// Use a named profile from config (defines which modes to enable).
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Custom prompt text for the search input (mainly useful in dmenu mode).
    #[arg(long, short = 'p', value_name = "TEXT")]
    pub prompt: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the IPC daemon. Same as `owlry -d`.
    Daemon,

    /// dmenu-compatible mode: read stdin, print the selection to stdout.
    Dmenu {
        /// Prompt text shown in the search input.
        #[arg(long, short = 'p', value_name = "TEXT")]
        prompt: Option<String>,
    },

    /// Diagnostics: providers, daemon socket, config status.
    Doctor,

    /// List providers (or show details for one by id).
    Providers {
        /// Provider id (e.g. `app`, `uuctl`, `power`). Optional.
        id: Option<String>,
    },

    /// Config tools.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Migrate TOML config to init.lua. Stub in 2.0; lands with Phase 3 (Lua config).
    MigrateConfig,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Parse config.toml and report any errors.
    Validate,
    /// Print the resolved effective config (defaults merged with user file).
    Show,
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
        assert!(args.command.is_none());
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
        assert!(CliArgs::try_parse_from(["owlry", "-d", "-m", "app"]).is_err());
        assert!(CliArgs::try_parse_from(["owlry", "-d", "--profile", "dev"]).is_err());
        assert!(CliArgs::try_parse_from(["owlry", "-d", "-p", "prompt"]).is_err());
    }

    #[test]
    fn daemon_flag_with_subcommand_resolves_to_daemon_in_dispatcher() {
        // clap allows both; main.rs checks args.daemon first and dispatches to
        // run_daemon, ignoring the subcommand. Encoded here so the precedence
        // can't drift silently.
        let args = CliArgs::try_parse_from(["owlry", "-d", "doctor"]).unwrap();
        assert!(args.daemon);
        assert!(args.command.is_some());
    }

    #[test]
    fn mode_flag_parses_known_providers() {
        let args = CliArgs::try_parse_from(["owlry", "-m", "app"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Application));

        let args = CliArgs::try_parse_from(["owlry", "-m", "cmd"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Command));

        let args = CliArgs::try_parse_from(["owlry", "-m", "dmenu"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Dmenu));

        let args = CliArgs::try_parse_from(["owlry", "-m", "uuctl"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Plugin("uuctl".into())));

        // Power aliases all resolve to Plugin("power") per D13.
        let args = CliArgs::try_parse_from(["owlry", "-m", "power"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Plugin("power".into())));
        let args = CliArgs::try_parse_from(["owlry", "-m", "sys"]).unwrap();
        assert_eq!(args.mode, Some(ProviderType::Plugin("power".into())));
    }

    #[test]
    fn plugin_subcommand_is_no_longer_recognised() {
        // The entire `plugin` subcommand tree was dropped in v2.
        assert!(CliArgs::try_parse_from(["owlry", "plugin", "list"]).is_err());
        assert!(CliArgs::try_parse_from(["owlry", "plugin", "install", "x"]).is_err());
    }

    #[test]
    fn daemon_subcommand_parses() {
        let args = CliArgs::try_parse_from(["owlry", "daemon"]).unwrap();
        assert!(matches!(args.command, Some(Command::Daemon)));
    }

    #[test]
    fn dmenu_subcommand_with_prompt_parses() {
        let args = CliArgs::try_parse_from(["owlry", "dmenu", "-p", "Pick:"]).unwrap();
        match args.command {
            Some(Command::Dmenu { prompt }) => assert_eq!(prompt.as_deref(), Some("Pick:")),
            other => panic!("expected Dmenu, got {other:?}"),
        }
    }

    #[test]
    fn doctor_subcommand_parses() {
        let args = CliArgs::try_parse_from(["owlry", "doctor"]).unwrap();
        assert!(matches!(args.command, Some(Command::Doctor)));
    }

    #[test]
    fn providers_subcommand_with_optional_id() {
        let args = CliArgs::try_parse_from(["owlry", "providers"]).unwrap();
        match args.command {
            Some(Command::Providers { id }) => assert!(id.is_none()),
            other => panic!("expected Providers, got {other:?}"),
        }
        let args = CliArgs::try_parse_from(["owlry", "providers", "uuctl"]).unwrap();
        match args.command {
            Some(Command::Providers { id }) => assert_eq!(id.as_deref(), Some("uuctl")),
            other => panic!("expected Providers, got {other:?}"),
        }
    }

    #[test]
    fn config_validate_subcommand_parses() {
        let args = CliArgs::try_parse_from(["owlry", "config", "validate"]).unwrap();
        assert!(matches!(
            args.command,
            Some(Command::Config {
                action: ConfigAction::Validate
            })
        ));
    }

    #[test]
    fn config_show_subcommand_parses() {
        let args = CliArgs::try_parse_from(["owlry", "config", "show"]).unwrap();
        assert!(matches!(
            args.command,
            Some(Command::Config {
                action: ConfigAction::Show
            })
        ));
    }

    #[test]
    fn migrate_config_subcommand_parses() {
        let args = CliArgs::try_parse_from(["owlry", "migrate-config"]).unwrap();
        assert!(matches!(args.command, Some(Command::MigrateConfig)));
    }
}
