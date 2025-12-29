mod app;
mod cli;
mod config;
mod data;
mod filter;
mod paths;
mod providers;
mod theme;
mod ui;

use app::OwlryApp;
use cli::CliArgs;
use log::{info, warn};

#[cfg(feature = "dev-logging")]
use log::debug;

fn main() {
    let default_level = if cfg!(feature = "dev-logging") { "debug" } else { "info" };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp_millis()
        .init();

    let args = CliArgs::parse_args();

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
