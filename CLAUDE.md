# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

The v2 rewrite is the load-bearing context here. Read [`docs/RESTRUCTURE-V2.md`](docs/RESTRUCTURE-V2.md) first if you're picking the project back up — it captures every design decision (D1–D21), the phased plan, and the per-task commit log.

## Project shape

Owlry is a single-binary Wayland launcher. Workspace has exactly one member:

```
crates/owlry/                  -- everything
├── Cargo.toml                 -- one binary + one library, features per optional provider
├── src/
│   ├── main.rs                -- subcommand router
│   ├── cli.rs                 -- clap definitions
│   ├── lib.rs                 -- module re-exports for integration tests
│   ├── server.rs              -- IPC daemon (bind + accept loop)
│   ├── client.rs              -- IPC client (UI mode)
│   ├── backend.rs             -- SearchBackend abstraction
│   ├── app.rs                 -- GTK4 setup
│   ├── ui/                    -- GTK widgets
│   │   ├── main_window.rs     -- MainWindow struct + construction
│   │   ├── main_window/       -- behavior split out of main_window.rs (2.5)
│   │   │   ├── launch.rs      -- activation / launching / submenu entry
│   │   │   ├── navigation.rs  -- key handling, tab + selection movement
│   │   │   ├── results.rs     -- result list paging
│   │   │   └── signals.rs     -- widget signal wiring
│   │   ├── actions.rs         -- contextual actions per result (Right Arrow)
│   │   ├── provider_meta.rs   -- per-provider icons / labels
│   │   ├── result_row.rs      -- one row widget
│   │   └── submenu.rs         -- submenu decode (incl. legacy SUBMENU: path)
│   ├── config/                -- TOML + Lua config loader; defines LoadedConfig
│   ├── lua/                   -- Lua config layer (feature: lua)
│   │   ├── api.rs             -- owlry.set / providers / tabs / provider / theme / profiles
│   │   ├── config.rs          -- LuaConfig accumulator + merge_into(Config)
│   │   ├── error.rs           -- LuaConfigError (thiserror)
│   │   ├── migrate.rs         -- TOML → owlry.lua serializer (deterministic)
│   │   ├── provider.rs        -- LuaProvider impl Provider
│   │   ├── runtime.rs         -- LuaContext (Arc<Lua>) + eval_file/snapshot
│   │   ├── util.rs            -- owlry.util.{shell, read_file, glob, env, hostname, …}
│   │   ├── validate.rs        -- ValidationReport with categorised findings
│   │   └── watcher.rs         -- notify-based hot reload (desktop-notification errors)
│   ├── data/                  -- FrecencyStore
│   ├── filter.rs              -- ProviderFilter + prefix parser
│   ├── ipc.rs                 -- Request/Response types
│   ├── paths.rs               -- XDG paths (honours $OWLRY_SOCKET)
│   ├── commands.rs            -- subcommand dispatchers (doctor / providers / config / …)
│   └── providers/             -- ALL providers
│       ├── mod.rs             -- Provider/DynamicProvider traits, ProviderManager
│       ├── application.rs     (core, always compiled)
│       ├── command.rs         (core)
│       ├── calculator.rs      (core)
│       ├── converter/         (core)
│       ├── power.rs           (core — pre-v2 name: sys/system)
│       ├── dmenu.rs           (core)
│       ├── clipboard.rs       (feature: clipboard)
│       ├── emoji.rs           (feature: emoji)
│       ├── rofimoji.rs        (feature: rofimoji — see docs/rofimoji.md)
│       ├── ssh.rs             (feature: ssh)
│       ├── systemd.rs         (feature: systemd — type_id "uuctl")
│       ├── websearch.rs       (feature: websearch)
│       └── filesearch.rs      (feature: filesearch)
```

There is no `crates/owlry-core`, `owlry-plugin-api`, `owlry-lua`, or `owlry-rune`. They were deleted in the v2 demolition. There is no `~/.config/owlry/plugins/` discovery directory and no `/usr/lib/owlry/plugins/*.so` loading — every "plugin" is a module compiled into `owlry`, core or feature-gated.

## Build & development

```bash
# Default features (minimal). App/cmd/calc/conv/power/dmenu are CORE — always
# compiled, not feature-gated. `default = ["systemd"]` is the only opt-in.
cargo build

# Full features (matches AUR build) — includes lua + every optional provider
cargo build --release --features full

# Lua-only opt-in on top of the default minimal set
cargo build --features lua

# Tests
cargo test --features full           # 450+ tests including lua + watcher + migrate
cargo test --no-default-features

# Verbose dev logging
cargo run -- --features dev-logging -- -d

# Format + lint
cargo fmt --all
cargo clippy --features full
cargo clippy --no-default-features    # must also stay silent

# Full quality gate — what CI runs (.gitea/workflows/ci.yml)
just verify                           # fmt-check + check + test + audit

# Install locally (sudo)
just install-local
```

The toolchain is pinned in `rust-toolchain.toml` (currently 1.97.1) so
rustfmt output stays stable across contributors. The AUR clean-chroot build
ignores it (Arch `rust` + `RUSTUP_TOOLCHAIN=stable`).

The `lua` cargo feature pulls in `mlua` (vendored Lua 5.4), `glob` (for
`owlry.util.glob`), and `notify` (for hot reload). It's part of the `full`
set; minimal builds (`cargo install` without flags) compile without it
and the `migrate-config` / Lua-load paths gracefully fall back to TOML.

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
owlry selector [-p <prompt>]   selector protocol mode (rofimoji et al.)
owlry doctor                   config + socket + providers status
owlry providers [<id>]         list providers (or show one)
owlry config validate          parse config, report errors (1) and warnings (2)
owlry config show              print resolved effective config
owlry migrate-config [--force] TOML → owlry.lua (functional from 2.1; --force to overwrite)
```

The `owlry plugin ...` subcommand tree from 1.x is **gone** in 2.0. Nothing to install, manage, or run via the CLI.

## Lua config layer (2.1+)

`~/.config/owlry/owlry.lua` is the canonical config from 2.1 onwards.
`config.toml` remains supported but is ignored entirely when `owlry.lua`
exists (an info log at daemon start surfaces this). The daemon spawns a
`notify` watcher on the config dir; saves are debounced (200ms),
re-evaluated in a fresh `LuaContext`, and hot-swapped atomically. Eval
failures are kept off the live state and surfaced via both the journal
**and** a `notify-rust` desktop notification with the precise mlua
line/column. See [`docs/lua-api.md`](docs/lua-api.md) for the full API
surface (`owlry.set`, `owlry.providers`, `owlry.tabs`, `owlry.provider`,
`owlry.theme`, `owlry.profiles`, `owlry.util.*`).

Implementation notes worth keeping straight:
- `LuaContext::lua: Arc<Lua>` — `mlua::Function` references don't bump
  the Lua refcount, so user providers hold their own `Arc<Lua>` clone
  via `LuaContext::lua_handle()` to outlive the context.
- `LoadedConfig` (in `config/mod.rs`) is the resolution result that
  keeps the `LuaContext` alive past `Config::load`. The daemon uses
  this; `Config::load` itself drops the context after merging scalars
  (so it can't be used to wire user providers — only `LoadedConfig`).
- `owlry config validate` runs `lua::validate::validate` against the
  snapshot and exits 0 / 1 / 2 per `docs/lua-api.md` §8.

## Provider model

Two traits in `src/providers/mod.rs`:

- **`Provider`** — static providers (apps, commands, power, clipboard, emoji, rofimoji, ssh, systemd). Populate `self.items` in `refresh()`, return `&self.items` from `items()`. Optional: `prefix()`, `icon()`, `tab_label()`, `search_noun()`, `position()`, `priority()`, `submenu_actions(data)`, `execute_action(command)`.
- **`DynamicProvider`** — per-keystroke providers (calculator, converter, filesearch, websearch). Generate items in `query(text)`. No `refresh`/`items` cache.

`ProviderManager::new_with_config` is the canonical registration site. Core providers (app, cmd, calc, conv, power, dmenu) are always compiled and gated only by their `[providers]` config flag; optional ones (clipboard, emoji, rofimoji, filesearch, ssh, systemd, websearch) are gated by both a cargo feature (compile-time) and the config flag (runtime).

## Result activation (typed since 2.5)

`LaunchItem::activation` is an `ItemActivation` enum — `Launch`,
`Submenu { provider_id, data }`, or `PluginAction { command }` — carried
typed across provider → IPC (`Request::Submenu` / `Request::PluginAction`)
→ backend → UI. The old `SUBMENU:<type_id>:<data>` string encoding survives
only as a decode path in `ui/submenu.rs` and `providers/mod.rs` for older
peers and script providers; the systemd provider still emits it.

`ui/actions.rs` builds the contextual action list opened with Right Arrow on
a launchable result (open-with, run-in-terminal, file/ssh actions). Plain
Enter keeps the item's primary activation. Actions are gated on
`ItemSource::Core` — script (Lua) providers get the primary action only.

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
