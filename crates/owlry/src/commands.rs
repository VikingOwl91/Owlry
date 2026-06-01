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

/// Print which config source is active (Lua / TOML / defaults) and nudge
/// the user toward the canonical 2.1+ format when they're still on TOML.
/// Also surfaces pre-v2 plugin / scripts artifacts in the same pass so the
/// user sees everything in one place rather than only on `migrate-config`.
fn print_config_source_info(out: &mut impl Write) {
    let lua_path = paths::lua_config_file();
    let toml_path = paths::config_file();

    let lua_exists = lua_path.as_deref().is_some_and(|p| p.exists());
    let toml_exists = toml_path.as_deref().is_some_and(|p| p.exists());

    #[cfg(feature = "lua")]
    {
        if lua_exists {
            let _ = writeln!(
                out,
                "  source: Lua ({})",
                lua_path.as_ref().unwrap().display()
            );
            if toml_exists {
                let _ = writeln!(
                    out,
                    "  note:   {} is present alongside owlry.lua — ignored.",
                    toml_path.as_ref().unwrap().display()
                );
            }
        } else if toml_exists {
            let _ = writeln!(
                out,
                "  source: TOML ({})",
                toml_path.as_ref().unwrap().display()
            );
            let _ = writeln!(
                out,
                "  hint:   `owlry migrate-config` rewrites it as owlry.lua, the canonical"
            );
            let _ = writeln!(
                out,
                "          format from 2.1 onwards. TOML works until 3.0; the migrator is"
            );
            let _ = writeln!(
                out,
                "          deterministic and pass `--force` to overwrite."
            );
        } else {
            let _ = writeln!(out, "  source: built-in defaults (no user config found)");
        }

        // Legacy plugin/scripts artifacts — same scan migrate-config uses.
        if let Some(toml_p) = toml_path.as_deref() {
            let config_dir = toml_p.parent().unwrap_or_else(|| std::path::Path::new("."));
            let artifacts = crate::lua::migrate::scan_legacy_artifacts(config_dir);
            if !artifacts.is_empty() {
                let _ = writeln!(out);
                let _ = writeln!(
                    out,
                    "  legacy: {} pre-v2 {} detected in your config dir.",
                    artifacts.len(),
                    if artifacts.len() == 1 {
                        "artifact"
                    } else {
                        "artifacts"
                    }
                );
                for a in &artifacts {
                    use crate::lua::migrate::LegacyKind;
                    match &a.kind {
                        LegacyKind::Plugin { id, .. } => {
                            let _ = writeln!(
                                out,
                                "          - plugin `{}` at {}",
                                id,
                                a.path.display()
                            );
                        }
                        LegacyKind::ScriptsDir { entries } => {
                            let _ = writeln!(
                                out,
                                "          - scripts dir ({} {}) at {}",
                                entries.len(),
                                if entries.len() == 1 {
                                    "entry"
                                } else {
                                    "entries"
                                },
                                a.path.display()
                            );
                        }
                    }
                }
                let _ = writeln!(
                    out,
                    "          Run `owlry migrate-config` for paste-ready owlry.provider {{}} skeletons."
                );
            }
        }
    }

    #[cfg(not(feature = "lua"))]
    {
        if toml_exists {
            let _ = writeln!(
                out,
                "  source: TOML ({})",
                toml_path.as_ref().unwrap().display()
            );
            let _ = writeln!(
                out,
                "  note:   this build was compiled without the `lua` feature — owlry.lua is ignored."
            );
        } else {
            let _ = writeln!(out, "  source: built-in defaults (no user config found)");
        }
        let _ = lua_exists; // suppress unused-var on non-lua builds
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
            print_config_source_info(&mut out);
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

/// `owlry config validate`: parse the config file and report errors and
/// warnings per docs/lua-api.md §8.
///
/// Resolution order matches `Config::load`:
/// 1. If `owlry.lua` exists, evaluate it. Eval errors → exit 1. Otherwise
///    run [`crate::lua::validate::validate`] on the snapshot and surface
///    every warning category (unknown set/theme keys, unknown provider
///    ids, tabs ⊄ providers, duplicate provider ids, compiled-out
///    providers, unknown ids inside profiles).
/// 2. Otherwise fall back to the TOML check.
///
/// Exit codes: 0 clean, 1 errors (eval / TOML parse failure), 2
/// warnings-only.
pub fn run_config_validate() -> ! {
    #[cfg(feature = "lua")]
    {
        if let Some(lua_path) = paths::lua_config_file()
            && lua_path.exists()
        {
            let exit = run_config_validate_lua(&lua_path);
            std::process::exit(exit);
        }
    }

    // No owlry.lua → fall back to TOML.
    match Config::load() {
        Ok(_) => {
            println!("config: OK (TOML)");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("config: ERROR — {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "lua")]
fn run_config_validate_lua(lua_path: &std::path::Path) -> i32 {
    use crate::config::LoadedConfig;
    use crate::lua::validate;

    let loaded = match LoadedConfig::load_lua_path(lua_path) {
        Ok(lc) => lc,
        Err(e) => {
            // Eval failure — report the chain (mlua error has line/col info).
            let mut msg = e.to_string();
            let mut cur: Option<&dyn std::error::Error> = e.source();
            while let Some(c) = cur {
                msg.push_str(": ");
                msg.push_str(&c.to_string());
                cur = c.source();
            }
            eprintln!("config: ERROR — {}", msg);
            return 1;
        }
    };
    let snapshot = match loaded.lua.as_ref() {
        Some(ctx) => ctx.snapshot(),
        None => {
            eprintln!("config: ERROR — internal: Lua context missing after load");
            return 1;
        }
    };

    let report = validate::validate(&snapshot);

    if report.is_clean() {
        println!("config: OK (Lua, {})", lua_path.display());
        return 0;
    }

    println!("config: validating {}", lua_path.display());
    if !report.warnings.is_empty() {
        println!("  {} warning(s):", report.warnings.len());
        for w in &report.warnings {
            println!("    - {}", w);
        }
    }
    if !report.errors.is_empty() {
        println!("  {} error(s):", report.errors.len());
        for e in &report.errors {
            println!("    - {}", e);
        }
    }
    report.exit_code()
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

/// `owlry migrate-config [--force]`: TOML → `owlry.lua`.
///
/// Reads `$XDG_CONFIG_HOME/owlry/config.toml`, parses it (pre-v2 aliases
/// like `system` / `badge_sys` normalise to v2 names via the existing
/// serde aliases), and writes `$XDG_CONFIG_HOME/owlry/owlry.lua` with
/// the equivalent settings. Output is deterministic and minimal —
/// values matching `Config::default()` are omitted. See
/// `docs/lua-api.md` §9 for the mapping table.
///
/// Builds without `--features lua` fall through to a clear stub: the
/// migration generates Lua code, which is only meaningful when the
/// runtime is present.
pub fn run_migrate_config(force: bool) -> ! {
    #[cfg(feature = "lua")]
    {
        let exit_code = run_migrate_config_impl(force);
        std::process::exit(exit_code);
    }
    #[cfg(not(feature = "lua"))]
    {
        let _ = force;
        eprintln!(
            "migrate-config: this owlry was built without the `lua` feature.\n\
             Rebuild with `cargo build --features lua` (or `--features full`)\n\
             to use the migrator. TOML config remains the active format until\n\
             3.0; see docs/lua-api.md §11."
        );
        std::process::exit(2);
    }
}

#[cfg(feature = "lua")]
fn run_migrate_config_impl(force: bool) -> i32 {
    let toml_path = match paths::config_file() {
        Some(p) => p,
        None => {
            eprintln!("migrate-config: cannot resolve $XDG_CONFIG_HOME/owlry/config.toml");
            return 2;
        }
    };
    let lua_path = match paths::lua_config_file() {
        Some(p) => p,
        None => {
            eprintln!("migrate-config: cannot resolve $XDG_CONFIG_HOME/owlry/owlry.lua");
            return 2;
        }
    };

    let req = crate::lua::MigrateRequest {
        toml_path: &toml_path,
        lua_path: &lua_path,
        force,
    };

    match crate::lua::migrate::migrate(&req) {
        Ok(out) => {
            println!(
                "owlry migrate-config: wrote {} ({} bytes) from {}",
                out.lua_path.display(),
                out.bytes_written,
                toml_path.display(),
            );
            println!("owlry.lua now takes precedence over config.toml (see docs/lua-api.md §2).");
            report_legacy_artifacts(&out.legacy_artifacts);
            0
        }
        Err(crate::lua::MigrateError::DestExists(_)) => {
            eprintln!(
                "migrate-config: {} already exists.\n\
                 Pass --force to overwrite, or delete it first.",
                lua_path.display()
            );
            1
        }
        Err(crate::lua::MigrateError::NoToml(_)) => {
            eprintln!(
                "migrate-config: no TOML config at {}.\n\
                 Nothing to migrate — write an owlry.lua directly, or\n\
                 copy /usr/share/doc/owlry/config.example.toml as a starting point.",
                toml_path.display()
            );
            1
        }
        Err(e) => {
            eprintln!("migrate-config: {}", e);
            2
        }
    }
}

/// Print a heads-up block for each pre-v2 artifact the migrator detected
/// in the config dir. Plugins (Rune / C-ABI) and the `scripts/` directory
/// can't be auto-translated — the user has to recreate them as
/// `owlry.provider {}` entries inside `owlry.lua`. We surface what we
/// found, including the original prefix / icon when known, so the user
/// has a paste-able starting point.
#[cfg(feature = "lua")]
fn report_legacy_artifacts(artifacts: &[crate::lua::migrate::LegacyArtifact]) {
    if artifacts.is_empty() {
        return;
    }
    use crate::lua::migrate::LegacyKind;

    println!();
    println!(
        "Heads up: detected {} pre-v2 {} in your config dir that the migrator",
        artifacts.len(),
        if artifacts.len() == 1 {
            "artifact"
        } else {
            "artifacts"
        }
    );
    println!("did NOT translate. They aren't loaded by 2.1+ — they're just files sitting");
    println!("there from your 1.x install. Recreate them in owlry.lua if you still use them:");

    for artifact in artifacts {
        println!();
        match &artifact.kind {
            LegacyKind::Plugin {
                id,
                name,
                prefix,
                icon,
                entry_point,
            } => {
                println!("  ── plugin: {} ─────────────────────────────────", id);
                println!("     path: {}", artifact.path.display());
                if let Some(n) = name {
                    println!("     name: {}", n);
                }
                if let Some(p) = prefix {
                    println!("     prefix: {}", p);
                }
                if let Some(i) = icon {
                    println!("     icon: {}", i);
                }
                if let Some(ep) = entry_point {
                    println!(
                        "     entry_point: {} (Rune / C-ABI; unsupported in 2.1)",
                        ep
                    );
                }
                println!("     ──");
                println!("     Recreate as a user provider in owlry.lua:");
                println!();
                println!("       owlry.provider {{");
                println!("         id = \"{}\",", id);
                if let Some(p) = prefix {
                    println!("         prefix = \"{}\",", p);
                }
                if let Some(i) = icon {
                    println!("         icon = \"{}\",", i);
                }
                println!("         items = function()");
                println!(
                    "           return {{ -- translate {} here",
                    entry_point.as_deref().unwrap_or("the plugin")
                );
                println!("             -- {{ name = \"…\", command = \"…\" }},");
                println!("           }}");
                println!("         end,");
                println!("       }}");
            }
            LegacyKind::ScriptsDir { entries } => {
                println!("  ── legacy `scripts/` directory ─────────────────");
                println!("     path: {}", artifact.path.display());
                println!(
                    "     {} {}: {}",
                    entries.len(),
                    if entries.len() == 1 { "file" } else { "files" },
                    if entries.is_empty() {
                        "(empty)".to_string()
                    } else {
                        entries.join(", ")
                    }
                );
                println!("     ──");
                println!("     The standalone `scripts` provider was removed in 2.0. Wrap each");
                println!("     script in an `owlry.provider {{}}` block, e.g.:");
                println!();
                println!("       owlry.provider {{");
                println!("         id = \"my-scripts\",");
                println!("         prefix = \":sh\",");
                println!("         items = function()");
                println!("           local items = {{}}");
                println!(
                    "           for _, name in ipairs(owlry.util.shell_lines(\"ls {}\")) do",
                    artifact.path.display()
                );
                println!("             table.insert(items, {{");
                println!("               name = name,");
                println!(
                    "               command = \"{}/\" .. name,",
                    artifact.path.display()
                );
                println!("             }})");
                println!("           end");
                println!("           return items");
                println!("         end,");
                println!("       }}");
            }
        }
    }
    println!();
    println!("  See `docs/lua-api.md` §4.4 (user providers) and §5.1 (owlry.util.*).");
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
