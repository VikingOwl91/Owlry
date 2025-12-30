//! Command-line interface for owlry launcher
//!
//! Provides both the launcher interface and plugin management commands.

use clap::{Parser, Subcommand};

use crate::providers::ProviderType;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "owlry",
    about = "An owl-themed application launcher for Wayland",
    version
)]
pub struct CliArgs {
    /// Start in single-provider mode (app, cmd, uuctl)
    #[arg(long, short = 'm', value_parser = parse_provider)]
    pub mode: Option<ProviderType>,

    /// Comma-separated list of enabled providers (app,cmd,uuctl)
    #[arg(long, short = 'p', value_delimiter = ',', value_parser = parse_provider)]
    pub providers: Option<Vec<ProviderType>>,

    /// Subcommand to run (if any)
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Manage plugins
    #[command(subcommand)]
    Plugin(PluginCommand),
}

/// Plugin runtime type
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PluginRuntime {
    /// Lua runtime (requires owlry-lua package)
    Lua,
    /// Rune runtime (requires owlry-rune package)
    Rune,
}

impl std::fmt::Display for PluginRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginRuntime::Lua => write!(f, "lua"),
            PluginRuntime::Rune => write!(f, "rune"),
        }
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommand {
    /// List installed plugins
    List {
        /// Show only enabled plugins
        #[arg(long)]
        enabled: bool,

        /// Show only disabled plugins
        #[arg(long)]
        disabled: bool,

        /// Filter by runtime type (lua or rune)
        #[arg(long, short = 'r', value_enum)]
        runtime: Option<PluginRuntime>,

        /// Show available plugins from registry instead of installed
        #[arg(long)]
        available: bool,

        /// Force refresh of registry cache
        #[arg(long)]
        refresh: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Search for plugins in the registry
    Search {
        /// Search query (matches name, description, tags)
        query: String,

        /// Force refresh of registry cache
        #[arg(long)]
        refresh: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Show detailed information about a plugin
    Info {
        /// Plugin ID
        name: String,

        /// Show info from registry instead of installed plugin
        #[arg(long)]
        registry: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Install a plugin from registry, path, or URL
    Install {
        /// Plugin source (registry name, local path, or git URL)
        source: String,

        /// Force reinstall even if already installed
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Remove an installed plugin
    Remove {
        /// Plugin ID to remove
        name: String,

        /// Don't ask for confirmation
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Update installed plugins
    Update {
        /// Specific plugin to update (all if not specified)
        name: Option<String>,
    },

    /// Enable a disabled plugin
    Enable {
        /// Plugin ID to enable
        name: String,
    },

    /// Disable an installed plugin
    Disable {
        /// Plugin ID to disable
        name: String,
    },

    /// Create a new plugin from template
    Create {
        /// Plugin ID (directory name)
        name: String,

        /// Runtime type to use (default: lua)
        #[arg(long, short = 'r', value_enum, default_value = "lua")]
        runtime: PluginRuntime,

        /// Target directory (default: current directory)
        #[arg(long, short = 'd')]
        dir: Option<String>,

        /// Plugin display name
        #[arg(long)]
        display_name: Option<String>,

        /// Plugin description
        #[arg(long)]
        description: Option<String>,
    },

    /// Validate a plugin's structure and manifest
    Validate {
        /// Path to plugin directory (default: current directory)
        path: Option<String>,
    },

    /// Show available script runtimes
    Runtimes,

    /// Run a plugin command
    ///
    /// Plugins can provide CLI commands that are invoked via:
    ///   owlry plugin run <plugin-id> <command> [args...]
    ///
    /// Example:
    ///   owlry plugin run bookmark add https://example.com "My Bookmark"
    Run {
        /// Plugin ID
        plugin_id: String,

        /// Command to run
        command: String,

        /// Arguments to pass to the command
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// List commands provided by a plugin
    Commands {
        /// Plugin ID (optional - lists all if not specified)
        plugin_id: Option<String>,
    },
}

fn parse_provider(s: &str) -> Result<ProviderType, String> {
    s.parse()
}

impl CliArgs {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
