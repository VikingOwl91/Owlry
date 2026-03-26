use log::{info, warn};

use owlry_core::paths;
use owlry_core::server::Server;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let sock = paths::socket_path();
    info!("Starting owlry-core daemon...");

    // Ensure the socket parent directory exists
    if let Err(e) = paths::ensure_parent_dir(&sock) {
        eprintln!("Failed to create socket directory: {e}");
        std::process::exit(1);
    }

    let server = match Server::bind(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start owlry-core: {e}");
            std::process::exit(1);
        }
    };

    // Graceful shutdown on SIGTERM/SIGINT
    let sock_cleanup = sock.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = std::fs::remove_file(&sock_cleanup);
        std::process::exit(0);
    }) {
        warn!("Failed to set signal handler: {}", e);
    }

    if let Err(e) = server.run() {
        eprintln!("Server error: {e}");
        std::process::exit(1);
    }
}
