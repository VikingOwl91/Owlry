# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

The v2 rewrite is the load-bearing context here. Read [`docs/RESTRUCTURE-V2.md`](docs/RESTRUCTURE-V2.md) first if you're picking the project back up — it captures every design decision (D1–D21), the phased plan, and the per-task commit log.

## Project shape

Owlry is a single-binary Wayland launcher. Workspace has exactly one member:

```
crates/owlry/                  -- everything
├── Cargo.toml                 -- one binary + one library, features per provider
├── src/
│   ├── main.rs                -- subcommand router
│   ├── cli.rs                 -- clap definitions
│   ├── lib.rs                 -- module re-exports for integration tests
│   ├── server.rs              -- IPC daemon (bind + accept loop)
│   ├── client.rs              -- IPC client (UI mode)
│   ├── backend.rs             -- SearchBackend abstraction
│   ├── app.rs                 -- GTK4 setup
│   ├── ui/                    -- GTK widgets
│   ├── config/                -- TOML config loader
│   ├── data/                  -- FrecencyStore
│   ├── filter.rs              -- ProviderFilter + prefix parser
│   ├── ipc.rs                 -- Request/Response types
│   ├── paths.rs               -- XDG paths (honours $OWLRY_SOCKET)
│   ├── commands.rs            -- subcommand dispatchers (doctor / providers / config / …)
│   └── providers/             -- ALL providers
│       ├── mod.rs             -- Provider/DynamicProvider traits, ProviderManager
│       ├── application.rs     (feature: app)
│       ├── command.rs         (feature: cmd)
│       ├── calculator.rs      (feature: calc)
│       ├── converter/         (feature: conv)
│       ├── power.rs           (feature: power — pre-v2 name: sys/system)
│       ├── dmenu.rs           (feature: dmenu)
│       ├── bookmarks.rs       (feature: bookmarks)
│       ├── clipboard.rs       (feature: clipboard)
│       ├── emoji.rs           (feature: emoji)
│       ├── ssh.rs             (feature: ssh)
│       ├── systemd.rs         (feature: systemd — type_id "uuctl")
│       ├── websearch.rs       (feature: websearch)
│       └── filesearch.rs      (feature: filesearch)
```

There is no `crates/owlry-core`, `owlry-plugin-api`, `owlry-lua`, or `owlry-rune`. They were deleted in the v2 demolition. There is no `~/.config/owlry/plugins/` discovery directory and no `/usr/lib/owlry/plugins/*.so` loading — every "plugin" is a feature-gated module compiled into `owlry`.

## Build & development

```bash
# Default features (minimal — app, cmd, calc, conv, power, dmenu)
cargo build

# Full features (matches AUR build)
cargo build --release --features full

# Tests
cargo test --workspace --features full     # 252+ tests
cargo test --workspace --no-default-features

# Verbose dev logging
cargo run -- --features dev-logging -- -d

# Format + lint
cargo fmt --all
cargo clippy --workspace --features full

# Install locally (sudo)
just install-local
```

The daemon binary is `owlry`. There is no separate `owlryd` binary anywhere — the daemon is `owlry -d` (or `owlry daemon`). The systemd user unit is `owlry.service` (pre-2.0 name: `owlryd.service`).

## Running locally without disturbing prod

For side-by-side testing against a production daemon, set `$OWLRY_SOCKET`:

```bash
OWLRY_SOCKET=/tmp/owlry-dev.sock target/release/owlry -d &
OWLRY_SOCKET=/tmp/owlry-dev.sock target/release/owlry -m uuctl
pkill -f 'target/release/owlry -d'
```

`$OWLRY_SOCKET` overrides `$XDG_RUNTIME_DIR/owlry/owlry.sock` for both the daemon (bind path) and the UI client (connect path).

## CLI shape

```
owlry                          UI, auto mode (default)
owlry -m <mode>                UI, single-provider mode
owlry --profile <name>         UI, named profile from config
owlry -p <prompt>              custom prompt text

owlry daemon                   run the daemon (alias: -d)
owlry dmenu [-p <prompt>]      dmenu mode (reads stdin, prints selection)
owlry doctor                   config + socket + providers status
owlry providers [<id>]         list providers (or show one)
owlry config validate          parse config, report errors
owlry config show              print resolved effective config
owlry migrate-config           TOML → init.lua (stub; lands with Lua config)
```

The `owlry plugin ...` subcommand tree from 1.x is **gone** in 2.0. Nothing to install, manage, or run via the CLI.

## Provider model

Two traits in `src/providers/mod.rs`:

- **`Provider`** — static providers (apps, commands, power, bookmarks, clipboard, emoji, ssh, systemd). Populate `self.items` in `refresh()`, return `&self.items` from `items()`. Optional: `prefix()`, `icon()`, `tab_label()`, `search_noun()`, `position()`, `priority()`, `submenu_actions(data)`, `execute_action(command)`.
- **`DynamicProvider`** — per-keystroke providers (calculator, converter, filesearch, websearch). Generate items in `query(text)`. No `refresh`/`items` cache.

`ProviderManager::new_with_config` is the canonical registration site. Each provider is gated by both a cargo feature (compile-time) and a config flag in `[providers]` (runtime).

## Submenu protocol

A provider returns items whose `command` field looks like `SUBMENU:<type_id>:<data>`. When the user selects one, the UI parses out `(plugin_id, data)`, sends a `Request::Submenu { plugin_id, data }`, and the daemon routes that to `Provider::submenu_actions(data)` on the matching provider. The systemd provider uses this for service start/stop/restart/enable/disable/status/journal actions.

For now the protocol is string-encoded; a typed redesign is on the roadmap.

## v2 naming rules

| Old (pre-v2) | New (2.0+) | Notes |
|---|---|---|
| `owlryd` binary | `owlry -d` | Single binary |
| `owlryd.service` / `owlryd.socket` | `owlry.service` / `owlry.socket` | `.install` hook handles upgrade |
| `:sys` / `:system` / "System" provider | `:power` / "Power" provider | `:sys` and `:system` kept as aliases |
| `providers.system = true` (config) | `providers.power = true` | Old key still accepted via serde alias |
| `badge_sys` (theme color) | `badge_power` | Old key aliased; CSS var `--owlry-badge-sys` still emitted for transition |
| `Plugin("sys")` type_id | `Plugin("power")` type_id | CLI mode parsing maps all three to `power` |

## Frecency

`~/.local/share/owlry/frecency.json`. Auto-saved every 5 min by the daemon; flushed on SIGTERM/SIGINT/SIGHUP. Boost weight via `providers.frecency_weight` (0.0 disabled, 1.0 strong).

## When you need to verify behavior live

1. `cargo build --release --features full`
2. `OWLRY_SOCKET=/tmp/owlry-dev.sock target/release/owlry -d &`
3. Run smoke queries via socket OR the UI with `OWLRY_SOCKET` set
4. `pkill -f 'target/release/owlry -d'`

Don't fight the prod daemon for the default socket path during testing. The env var exists for exactly this.
