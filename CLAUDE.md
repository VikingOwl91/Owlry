# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
just build          # Debug build (all workspace members)
just build-ui       # UI binary only
just build-daemon   # Core daemon only
just release        # Release build (LTO, stripped)
just release-daemon # Release build for daemon only
just check          # cargo check + clippy
just test           # Run tests
just fmt            # Format code
just run [ARGS]     # Run UI with optional args (e.g., just run --mode app)
just run-daemon     # Run core daemon
just install-local  # Install core + daemon + runtimes + systemd units

# Dev build with verbose logging
cargo run -p owlry --features dev-logging

# Build core without embedded Lua (smaller binary, uses external owlry-lua)
cargo build -p owlry --release --no-default-features
```

## Usage Examples

### Basic Invocation

The UI client connects to the `owlry-core` daemon via Unix socket IPC. Start the daemon first:

```bash
# Start daemon (systemd recommended)
systemctl --user enable --now owlry-core.service

# Or run directly
owlry-core

# Then launch UI
owlry                      # Launch with all providers
owlry -m app               # Applications only
owlry -m cmd               # PATH commands only
owlry --profile dev        # Use a named config profile
owlry -m calc              # Calculator plugin only (if installed)
```

### dmenu Mode

dmenu mode runs locally without the daemon. Use `-m dmenu` with piped input for interactive selection. The selected item is printed to stdout (not executed), so pipe the output to execute it:

```bash
# Screenshot menu (execute selected command)
printf '%s\n' \
  "grimblast --notify copy screen" \
  "grimblast --notify copy area" \
  "grimblast --notify edit screen" \
  | owlry -m dmenu -p "Screenshot" \
  | sh

# Git branch checkout
git branch | owlry -m dmenu -p "checkout" | xargs git checkout

# Kill a process
ps -eo comm | sort -u | owlry -m dmenu -p "kill" | xargs pkill

# Select and open a project
find ~/projects -maxdepth 1 -type d | owlry -m dmenu | xargs code
```

### CLI Flags

| Flag | Description |
|------|-------------|
| `-m`, `--mode MODE` | Start in single-provider mode (app, cmd, dmenu, calc, etc.) |
| `--profile NAME` | Use a named profile from config (defines which modes to enable) |
| `-p`, `--prompt TEXT` | Custom prompt text for the search input (dmenu mode) |

### Available Modes

| Mode | Description |
|------|-------------|
| `app` | Desktop applications |
| `cmd` | PATH commands |
| `dmenu` | Pipe-based selection (requires stdin, runs locally) |
| `calc` | Calculator (plugin) |
| `clip` | Clipboard history (plugin) |
| `emoji` | Emoji picker (plugin) |
| `ssh` | SSH hosts (plugin) |
| `sys` | System actions (plugin) |
| `bm` | Bookmarks (plugin) |
| `file` | File search (plugin) |
| `web` | Web search (plugin) |
| `uuctl` | systemd user units (plugin) |

### Search Prefixes

Type these in the search box to filter by provider:

| Prefix | Provider | Example |
|--------|----------|---------|
| `:app` | Applications | `:app firefox` |
| `:cmd` | PATH commands | `:cmd git` |
| `:sys` | System actions | `:sys shutdown` |
| `:ssh` | SSH hosts | `:ssh server` |
| `:clip` | Clipboard | `:clip password` |
| `:bm` | Bookmarks | `:bm github` |
| `:emoji` | Emoji | `:emoji heart` |
| `:calc` | Calculator | `:calc sqrt(16)` |
| `:web` | Web search | `:web rust docs` |
| `:file` | Files | `:file config` |
| `:uuctl` | systemd | `:uuctl docker` |
| `:tag:X` | Filter by tag | `:tag:development` |

### Trigger Prefixes

| Trigger | Provider | Example |
|---------|----------|---------|
| `=` | Calculator | `= 5+3` |
| `?` | Web search | `? rust programming` |
| `/` | File search | `/ .bashrc` |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Launch selected item |
| `Escape` | Close launcher / exit submenu |
| `Up` / `Down` | Navigate results |
| `Tab` | Cycle filter tabs |
| `Shift+Tab` | Cycle tabs (reverse) |
| `Ctrl+1..9` | Toggle tab by position |

### Plugin CLI

```bash
owlry plugin list                  # List installed
owlry plugin list --available      # Show registry
owlry plugin search "query"        # Search registry
owlry plugin install <name>        # Install from registry
owlry plugin install ./path        # Install from local path
owlry plugin remove <name>         # Uninstall
owlry plugin enable/disable <name> # Toggle
owlry plugin create <name>         # Create Lua plugin template
owlry plugin create <name> -r rune # Create Rune plugin template
owlry plugin validate ./path       # Validate plugin structure
owlry plugin run <id> <cmd> [args] # Run plugin CLI command
owlry plugin commands <id>         # List plugin commands
owlry plugin runtimes              # Show available runtimes
```

## Release Workflow

Always use `just` for releases - do NOT manually edit Cargo.toml for version bumps:

```bash
# Bump a single crate
just bump-crate owlry-core 0.5.1

# Bump all crates to same version
just bump-all 0.5.1

# Bump core UI only
just bump 0.5.1

# Create and push release tag
git push && just tag

# AUR package management
just aur-update           # Update core UI PKGBUILD
just aur-update-pkg NAME  # Update specific package (owlry-core, owlry-lua, etc.)
just aur-update-all       # Update all AUR packages
just aur-publish          # Publish core UI to AUR
just aur-publish-all      # Publish all AUR packages

# Version inspection
just show-versions        # List all crate versions
just aur-status           # Show AUR package versions and git status
```

## AUR Packaging

The `aur/` directory contains PKGBUILDs for core packages:

| Category | Packages |
|----------|----------|
| Core UI | `owlry` |
| Core Daemon | `owlry-core` |
| Runtimes | `owlry-lua`, `owlry-rune` |
| Meta-bundles | `owlry-meta-essentials`, `owlry-meta-widgets`, `owlry-meta-tools`, `owlry-meta-full` |

Plugin AUR packages are in the separate `owlry-plugins` repo at `somegit.dev/Owlibou/owlry-plugins`.

## Architecture

### Client/Daemon Split

Owlry uses a client/daemon architecture:

- **`owlry`** (client): GTK4 UI that connects to the daemon via Unix socket IPC. Handles rendering, user input, and launching applications. In dmenu mode, runs a local `ProviderManager` directly (no daemon needed).
- **`owlry-core`** (daemon): Headless background service that loads plugins, manages providers, handles fuzzy matching, frecency scoring, and serves queries over IPC. Runs as a systemd user service.

### Workspace Structure

```
owlry/
├── Cargo.toml                      # Workspace root
├── systemd/                        # systemd user service/socket files
│   ├── owlry-core.service
│   └── owlry-core.socket
├── crates/
│   ├── owlry/                      # UI client binary (GTK4 + Layer Shell)
│   │   └── src/
│   │       ├── main.rs             # Entry point
│   │       ├── app.rs              # GTK Application setup, CSS loading
│   │       ├── cli.rs              # Clap CLI argument parsing
│   │       ├── client.rs           # CoreClient - IPC client to daemon
│   │       ├── backend.rs          # SearchBackend - abstraction over IPC/local
│   │       ├── theme.rs            # Theme loading
│   │       ├── plugin_commands.rs  # Plugin CLI subcommand handlers
│   │       ├── providers/          # dmenu provider (local-only)
│   │       └── ui/                 # GTK widgets (MainWindow, ResultRow, submenu)
│   ├── owlry-core/                 # Daemon library + binary
│   │   └── src/
│   │       ├── main.rs             # Daemon entry point
│   │       ├── lib.rs              # Public API (re-exports modules)
│   │       ├── server.rs           # Unix socket IPC server
│   │       ├── ipc.rs              # Request/Response message types
│   │       ├── filter.rs           # ProviderFilter - mode/prefix filtering
│   │       ├── paths.rs            # XDG path utilities, socket path
│   │       ├── notify.rs           # Desktop notifications
│   │       ├── config/             # Config loading (config.toml)
│   │       ├── data/               # FrecencyStore
│   │       ├── providers/          # Application, Command, native/lua provider hosts
│   │       └── plugins/            # Plugin loading, manifests, registry, runtimes
│   ├── owlry-plugin-api/           # ABI-stable plugin interface
│   ├── owlry-lua/                  # Lua script runtime (cdylib)
│   └── owlry-rune/                 # Rune script runtime (cdylib)
```

### IPC Protocol

Communication uses newline-delimited JSON over a Unix domain socket at `$XDG_RUNTIME_DIR/owlry/owlry.sock`.

**Request types** (`owlry_core::ipc::Request`):

| Type | Purpose |
|------|---------|
| `Query` | Search with text and optional mode filters |
| `Launch` | Record a launch event for frecency |
| `Providers` | List available providers |
| `Refresh` | Refresh a specific provider |
| `Toggle` | Toggle visibility (client-side concern, daemon acks) |
| `Submenu` | Query submenu actions for a plugin item |
| `PluginAction` | Execute a plugin action command |

**Response types** (`owlry_core::ipc::Response`):

| Type | Purpose |
|------|---------|
| `Results` | Search results with `Vec<ResultItem>` |
| `Providers` | Provider list with `Vec<ProviderDesc>` |
| `SubmenuItems` | Submenu actions for a plugin |
| `Ack` | Success acknowledgement |
| `Error` | Error with message |

### Core Data Flow

```
[owlry UI]                              [owlry-core daemon]

main.rs → CliArgs → OwlryApp            main.rs → Server::bind()
         ↓                                         ↓
    SearchBackend                        UnixListener accept loop
         ↓                                         ↓
  ┌──────┴──────┐                        handle_request()
  ↓             ↓                              ↓
Daemon       Local (dmenu)         ┌───────────┴───────────┐
  ↓                                ↓                       ↓
CoreClient ──── IPC ────→   ProviderManager         ProviderFilter
                                   ↓                       ↓
                         [Provider impls]         parse_query()
                                   ↓
                            LaunchItem[]
                                   ↓
                         FrecencyStore (boost)
                                   ↓
                         Response::Results ──── IPC ────→ UI rendering
```

### Provider System

**Core providers** (in `owlry-core`):
- **Application**: Desktop applications from XDG directories
- **Command**: Shell commands from PATH

**dmenu provider** (in `owlry` client, local only):
- **Dmenu**: Pipe-based input (dmenu compatibility)

All other providers are native plugins in the separate `owlry-plugins` repo (`somegit.dev/Owlibou/owlry-plugins`).

**User plugins** (script-based, in `~/.config/owlry/plugins/`):
- **Lua plugins**: Loaded by `owlry-lua` runtime from `/usr/lib/owlry/runtimes/liblua.so`
- **Rune plugins**: Loaded by `owlry-rune` runtime from `/usr/lib/owlry/runtimes/librune.so`
- User plugins are **hot-reloaded** automatically when files change (no daemon restart needed)
- Custom prefixes (e.g., `:hs`) are resolved dynamically for user plugins

`ProviderManager` (in `owlry-core`) orchestrates providers and handles:
- Fuzzy matching via `SkimMatcherV2`
- Frecency score boosting
- Native plugin loading from `/usr/lib/owlry/plugins/`
- Script runtime loading from `/usr/lib/owlry/runtimes/` for user plugins
- Filesystem watching for automatic user plugin hot-reload

**Submenu System**: Plugins can return items with `SUBMENU:plugin_id:data` commands. When selected, the plugin is queried with `?SUBMENU:data` to get action items (e.g., systemd service actions).

### Plugin API

Native plugins use the ABI-stable interface in `owlry-plugin-api`:

```rust
#[repr(C)]
pub struct PluginVTable {
    pub info: extern "C" fn() -> PluginInfo,
    pub providers: extern "C" fn() -> RVec<ProviderInfo>,
    pub provider_init: extern "C" fn(id: RStr) -> ProviderHandle,
    pub provider_refresh: extern "C" fn(ProviderHandle) -> RVec<PluginItem>,
    pub provider_query: extern "C" fn(ProviderHandle, RStr) -> RVec<PluginItem>,
    pub provider_drop: extern "C" fn(ProviderHandle),
}

// Each plugin exports:
#[no_mangle]
pub extern "C" fn owlry_plugin_vtable() -> &'static PluginVTable
```

Plugins are compiled as `.so` (cdylib) and loaded by the daemon at startup.

**Plugin locations** (when deployed):
- `/usr/lib/owlry/plugins/*.so` - Native plugins
- `/usr/lib/owlry/runtimes/*.so` - Script runtimes (liblua.so, librune.so)
- `~/.config/owlry/plugins/` - User plugins (Lua/Rune)

### Filter & Prefix System

`ProviderFilter` (`owlry-core/src/filter.rs`) handles:
- CLI mode selection (`--mode app`)
- Profile-based mode selection (`--profile dev`)
- Provider toggling (Ctrl+1/2/3)
- Prefix parsing (`:app`, `:cmd`, `:sys`, etc.)
- Dynamic prefix fallback for user plugins (any `:word` prefix maps to `Plugin(word)`)

Query parsing extracts prefix and forwards clean query to providers.

### SearchBackend

`SearchBackend` (`owlry/src/backend.rs`) abstracts over two modes:
- **`Daemon`**: Wraps `CoreClient`, sends queries over IPC to `owlry-core`
- **`Local`**: Wraps `ProviderManager` directly (used for dmenu mode only)

### UI Layer

- `MainWindow` (`src/ui/main_window.rs`): GTK4 window with Layer Shell overlay
- `ResultRow` (`src/ui/result_row.rs`): Individual result rendering
- `submenu` (`src/ui/submenu.rs`): Universal submenu parsing utilities (plugins provide actions)

### Configuration

`Config` (`owlry-core/src/config/mod.rs`) loads from `~/.config/owlry/config.toml`:
- Auto-detects terminal (`$TERMINAL` -> `xdg-terminal-exec` -> common terminals)
- Optional `use_uwsm = true` for systemd session integration (launches apps via `uwsm app --`)
- Profiles: Define named mode sets under `[profiles.<name>]` with `modes = ["app", "cmd", ...]`

### Theming

CSS loading priority (`owlry/src/app.rs`):
1. Base structural CSS (`resources/base.css`)
2. Theme CSS (built-in "owl" or custom `~/.config/owlry/themes/{name}.css`)
3. User overrides (`~/.config/owlry/style.css`)
4. Config variable injection

### Systemd Integration

Service files in `systemd/`:
- `owlry-core.service`: Runs daemon as `Type=simple`, restarts on failure
- `owlry-core.socket`: Socket activation at `%t/owlry/owlry.sock`

Start with: `systemctl --user enable --now owlry-core.service`

## Plugins

Plugins live in a separate repository: `somegit.dev/Owlibou/owlry-plugins`

13 native plugin crates, all compiled as cdylib (.so):

| Category | Plugins | Behavior |
|----------|---------|----------|
| Static | bookmarks, clipboard, emoji, scripts, ssh, system, systemd | Loaded at startup, refresh() populates items |
| Dynamic | calculator, websearch, filesearch | Queried per-keystroke via query() |
| Widget | weather, media, pomodoro | Displayed at top of results |

## Key Patterns

- **Rc<RefCell<T>>** used throughout for GTK signal handlers needing mutable state
- **Feature flag `dev-logging`**: Wraps debug!() calls in `#[cfg(feature = "dev-logging")]`
- **Feature flag `lua`**: Enables built-in Lua runtime (off by default); enable to embed Lua in core binary
- **Script runtimes**: External `.so` runtimes loaded from `/usr/lib/owlry/runtimes/` — Lua and Rune user plugins loaded from `~/.config/owlry/plugins/`
- **Hot-reload**: Filesystem watcher (`notify` crate) monitors user plugins dir and reloads runtimes on file changes
- **dmenu mode**: Runs locally without daemon. Use `-m dmenu` with piped stdin
- **Frecency**: Time-decayed frequency scoring stored in `~/.local/share/owlry/frecency.json`
- **ABI stability**: Plugin interface uses `abi_stable` crate for safe Rust dynamic linking
- **Plugin API v3**: Adds `position` (Normal/Widget) and `priority` fields to ProviderInfo
- **ProviderType simplification**: Core uses only `Application`, `Command`, `Dmenu`, `Plugin(String)` - all plugin-specific types removed from core

## Dependencies (Rust 1.90+, GTK 4.12+)

External tool dependencies (for plugins):
- Clipboard plugin: `cliphist`, `wl-clipboard`
- File search plugin: `fd` or `mlocate`
- Emoji plugin: `wl-clipboard`, `noto-fonts-emoji`
- Systemd plugin: `systemd` (user services)
- Bookmarks plugin: Firefox support uses `rusqlite` with bundled SQLite (no system dependency)
