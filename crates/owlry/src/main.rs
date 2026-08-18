use log::{info, warn};
use std::os::unix::io::AsRawFd;

use owlry::app::OwlryApp;
use owlry::cli::{CliArgs, Command};
use owlry::{client, commands, paths};

#[cfg(feature = "dev-logging")]
use log::debug;

/// Try to acquire an exclusive lock on the UI lock file.
///
/// Returns `Some(File)` if the lock was acquired (no other instance running),
/// or `None` if another instance already holds the lock.
/// The returned `File` must be kept alive for the duration of the process.
fn try_acquire_lock() -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    let lock_path = paths::socket_path().parent().unwrap().join("owlry-ui.lock");

    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&lock_path)
        .ok()
        .and_then(|f| {
            let fd = f.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret == 0 { Some(f) } else { None }
        })
}

fn main() {
    // Capture process-start as early as possible so the UI can measure the
    // pre-activate GTK/display/D-Bus init cost (see app::on_activate timing).
    let _ = owlry::app::BOOT.set(std::time::Instant::now());

    let args = CliArgs::parse_args();

    // Subcommand dispatch. Each commands::* function calls std::process::exit
    // and never returns. Daemon mode also installs env_logger; the other
    // subcommands stay quiet by default.
    if args.daemon {
        init_logger();
        commands::run_daemon();
    }

    match args.command.clone() {
        Some(Command::Daemon) => {
            init_logger();
            commands::run_daemon();
        }
        Some(Command::Doctor) => commands::run_doctor(),
        Some(Command::Providers { id }) => commands::run_providers(id),
        Some(Command::Config { action }) => commands::run_config(action),
        Some(Command::MigrateConfig { force }) => commands::run_migrate_config(force),
        Some(Command::Dmenu { prompt }) => {
            init_logger();
            commands::run_dmenu(prompt);
        }
        Some(Command::Selector { prompt }) => {
            init_logger();
            commands::run_selector(prompt);
        }
        None => {
            // Fall through to UI launch below.
        }
    }

    // Default: launch the UI.
    init_logger();

    #[cfg(feature = "dev-logging")]
    {
        debug!("┌─────────────────────────────────────────┐");
        debug!("│  DEV-LOGGING: Verbose output enabled    │");
        debug!("└─────────────────────────────────────────┘");
        debug!("CLI args: {:?}", args);
    }

    // Toggle behavior: if another instance is already running, tell the daemon
    // to toggle visibility and exit immediately.
    let _lock_guard = match try_acquire_lock() {
        Some(file) => file,
        None => {
            info!("Another owlry instance detected, sending toggle");
            let socket_path = client::CoreClient::socket_path();
            if let Ok(mut client) = client::CoreClient::connect(&socket_path) {
                if let Err(e) = client.toggle() {
                    eprintln!("Failed to toggle existing instance: {}", e);
                    std::process::exit(1);
                }
            } else {
                eprintln!("Another instance is running but daemon is unreachable");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
    };

    info!("Starting Owlry launcher");

    // Diagnostic: log critical environment variables.
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

fn init_logger() {
    let default_level = if cfg!(feature = "dev-logging") {
        "debug"
    } else {
        "info"
    };
    let _ =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
            .format_timestamp_millis()
            .try_init();
}
