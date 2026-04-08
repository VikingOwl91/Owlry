use log::info;

use owlry_core::paths;
use owlry_core::server::Server;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let sock = paths::socket_path();
    info!("Starting owlryd daemon...");

    // Ensure the socket parent directory exists
    if let Err(e) = paths::ensure_parent_dir(&sock) {
        eprintln!("Failed to create socket directory: {e}");
        std::process::exit(1);
    }

    let server = match Server::bind(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start owlryd: {e}");
            std::process::exit(1);
        }
    };

    // SIGTERM/SIGINT are handled inside Server::run() via signal-hook,
    // which saves frecency before exiting.

    if let Err(e) = server.run() {
        eprintln!("Server error: {e}");
        std::process::exit(1);
    }
}
