mod app;
pub mod client;
mod cli;
mod plugin_commands;
mod providers;
mod theme;
mod ui;

use app::OwlryApp;
use cli::{CliArgs, Command};
use log::{info, warn};

#[cfg(feature = "dev-logging")]
use log::debug;

fn main() {
    let args = CliArgs::parse_args();

    // Handle subcommands before initializing the full app
    if let Some(command) = &args.command {
        // CLI commands don't need full logging
        match command {
            Command::Plugin(plugin_cmd) => {
                if let Err(e) = plugin_commands::execute(plugin_cmd.clone()) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
        }
    }

    // No subcommand - launch the app
    let default_level = if cfg!(feature = "dev-logging") { "debug" } else { "info" };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp_millis()
        .init();

    #[cfg(feature = "dev-logging")]
    {
        debug!("┌─────────────────────────────────────────┐");
        debug!("│  DEV-LOGGING: Verbose output enabled    │");
        debug!("└─────────────────────────────────────────┘");
        debug!("CLI args: {:?}", args);
    }

    info!("Starting Owlry launcher");

    // Diagnostic: log critical environment variables
    let home = std::env::var("HOME").unwrap_or_else(|_| "<not set>".to_string());
    let path = std::env::var("PATH").unwrap_or_else(|_| "<not set>".to_string());
    let xdg_data = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| "<not set>".to_string());
    info!("HOME={}", home);
    info!("PATH={}", path);
    info!("XDG_DATA_HOME={}", xdg_data);

    if home == "<not set>" || path == "<not set>" {
        warn!("Critical environment variables missing! Items may not load correctly.");
    }

    let app = OwlryApp::new(args);
    std::process::exit(app.run());
}
