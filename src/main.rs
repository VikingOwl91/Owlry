mod app;
mod cli;
mod config;
mod filter;
mod providers;
mod ui;

use app::OwlryApp;
use cli::CliArgs;
use log::info;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = CliArgs::parse_args();

    info!("Starting Owlry launcher");

    let app = OwlryApp::new(args);
    std::process::exit(app.run());
}
