//! Implementations of CLI subcommands that don't launch the UI.
//!
//! Each function exits the process when done — they're invoked from `main.rs`
//! after subcommand dispatch and never return.

use std::io::{self, Write};

use crate::cli::ConfigAction;
use crate::client::CoreClient;
use crate::config::Config;
use crate::ipc::ProviderDesc;
use crate::{paths, server};

/// `owlry daemon` / `owlry -d`: bind the IPC socket and run the daemon loop.
pub fn run_daemon() -> ! {
    let sock = paths::socket_path();
    if let Err(e) = paths::ensure_parent_dir(&sock) {
        eprintln!("Failed to create socket directory: {e}");
        std::process::exit(1);
    }
    match server::Server::bind(&sock) {
        Ok(s) => match s.run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("Server error: {e}");
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Failed to start daemon: {e}");
            std::process::exit(1);
        }
    }
}

/// `owlry doctor`: print the daemon socket status, loaded provider list, and
/// config status. Useful for confirming an install is wired correctly.
pub fn run_doctor() -> ! {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let _ = writeln!(out, "owlry doctor");
    let _ = writeln!(out, "============");
    let _ = writeln!(out);

    // Config check.
    let _ = writeln!(out, "[config]");
    match Config::load() {
        Ok(_) => {
            let _ = writeln!(out, "  OK");
        }
        Err(e) => {
            let _ = writeln!(out, "  ERROR: {e}");
        }
    }
    let _ = writeln!(out);

    // Daemon socket.
    let sock = paths::socket_path();
    let _ = writeln!(out, "[daemon]");
    let _ = writeln!(out, "  socket path: {}", sock.display());
    match CoreClient::connect(&sock) {
        Ok(mut client) => {
            let _ = writeln!(out, "  socket: OK (daemon reachable)");
            match client.providers() {
                Ok(list) => {
                    let _ = writeln!(out);
                    let _ = writeln!(out, "[providers]  {} registered", list.len());
                    print_provider_list(&mut out, &list);
                }
                Err(e) => {
                    let _ = writeln!(out, "  providers query failed: {e}");
                }
            }
        }
        Err(e) => {
            let _ = writeln!(out, "  socket: UNREACHABLE ({e})");
            let _ = writeln!(
                out,
                "  hint: start the daemon with `owlry -d` or via systemd"
            );
        }
    }

    std::process::exit(0);
}

/// `owlry providers [<id>]`: list providers, or show one in detail.
pub fn run_providers(id: Option<String>) -> ! {
    let sock = paths::socket_path();
    let mut client = match CoreClient::connect(&sock) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to reach daemon socket {}: {}", sock.display(), e);
            eprintln!("Hint: start the daemon with `owlry -d`.");
            std::process::exit(1);
        }
    };

    let list = match client.providers() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Daemon providers query failed: {e}");
            std::process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();

    match id {
        None => {
            let _ = writeln!(out, "{} provider(s) registered", list.len());
            print_provider_list(&mut out, &list);
        }
        Some(target) => match list.iter().find(|p| p.id == target) {
            Some(p) => {
                let _ = writeln!(out, "id:          {}", p.id);
                let _ = writeln!(out, "name:        {}", p.name);
                let _ = writeln!(out, "icon:        {}", p.icon);
                let _ = writeln!(out, "position:    {}", p.position);
                if let Some(prefix) = &p.prefix {
                    let _ = writeln!(out, "prefix:      {prefix}");
                }
                if let Some(tab_label) = &p.tab_label {
                    let _ = writeln!(out, "tab_label:   {tab_label}");
                }
                if let Some(search_noun) = &p.search_noun {
                    let _ = writeln!(out, "search_noun: {search_noun}");
                }
            }
            None => {
                eprintln!("No provider with id '{target}'.");
                std::process::exit(1);
            }
        },
    }

    std::process::exit(0);
}

fn print_provider_list(out: &mut impl Write, list: &[ProviderDesc]) {
    for p in list {
        let prefix = p.prefix.as_deref().unwrap_or("-");
        let _ = writeln!(out, "  {:<12}  {:<22}  prefix={}", p.id, p.name, prefix);
    }
}

/// `owlry config validate`: parse the config file and report errors.
pub fn run_config_validate() -> ! {
    match Config::load() {
        Ok(_) => {
            println!("config: OK");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("config: ERROR — {e}");
            std::process::exit(1);
        }
    }
}

/// `owlry config show`: serialize the effective config to stdout as TOML.
pub fn run_config_show() -> ! {
    let cfg = Config::load_or_default();
    match toml::to_string_pretty(&cfg) {
        Ok(s) => {
            print!("{s}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("config: failed to serialize ({e})");
            std::process::exit(1);
        }
    }
}

pub fn run_config(action: ConfigAction) -> ! {
    match action {
        ConfigAction::Validate => run_config_validate(),
        ConfigAction::Show => run_config_show(),
    }
}

/// `owlry migrate-config`: TOML → init.lua. Lands in Phase 3 (Lua config).
pub fn run_migrate_config() -> ! {
    eprintln!(
        "migrate-config: not yet implemented.\n\
         The Lua config layer lands in 2.1+ (or 3.0). TOML config remains the\n\
         active format in 2.0. See docs/RESTRUCTURE-V2.md (phase 3) for status."
    );
    std::process::exit(2);
}

/// `owlry dmenu`: delegate to the UI's dmenu pipeline.
///
/// The UI reads stdin itself when launched in dmenu mode — we just construct
/// the equivalent of `owlry -m dmenu [-p prompt]` and hand off.
pub fn run_dmenu(prompt: Option<String>) -> ! {
    let args = crate::cli::CliArgs {
        daemon: false,
        mode: Some(crate::providers::ProviderType::Dmenu),
        profile: None,
        prompt,
        command: None,
    };
    let app = crate::app::OwlryApp::new(args);
    std::process::exit(app.run());
}
