//! Command-line interface for owlry launcher

use clap::Parser;

use owlry_core::providers::ProviderType;

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
