# Owlry Architecture Split — Design Spec

**Date:** 2026-03-26
**Status:** Draft
**Goal:** Split owlry into two repos with a client/daemon architecture, enabling independent release cadence and clean architectural separation.

---

## 1. Overview

Currently owlry is a monorepo with 18 crates that all share version `0.4.10`. Changing any single package requires bumping all versions and rebuilding every AUR package from the same source. The architecture also couples the GTK UI directly to plugin loading and provider management, meaning the UI blocks during initialization.

This redesign:
- Splits into two repositories (core + plugins)
- Extracts backend logic into `owlry-core` daemon
- Makes `owlry` a thin GTK4 client communicating over Unix socket IPC
- Enables independent versioning and release cadence per package
- Makes third-party plugin development straightforward

---

## 2. Repository Layout

### Repo 1 — `owlry/` (core, changes infrequently)

```
owlry/
├── Cargo.toml                      # Workspace: owlry, owlry-core, owlry-plugin-api
├── crates/
│   ├── owlry/                      # GTK4 frontend binary (thin UI client)
│   │   └── src/
│   │       ├── main.rs             # Entry point, CLI parsing
│   │       ├── app.rs              # GTK Application setup, CSS/theme loading
│   │       ├── cli.rs              # Argument parsing (--mode, --profile, --dmenu)
│   │       ├── client.rs           # IPC client (connects to daemon socket)
│   │       ├── ui/
│   │       │   ├── mod.rs
│   │       │   ├── main_window.rs  # GTK window, Layer Shell overlay
│   │       │   ├── result_row.rs   # Result rendering
│   │       │   └── submenu.rs      # Submenu rendering
│   │       ├── theme.rs            # CSS loading priority
│   │       └── providers/
│   │           └── dmenu.rs        # Self-contained dmenu mode (bypasses daemon)
│   ├── owlry-core/                 # Daemon binary
│   │   └── src/
│   │       ├── main.rs             # Daemon entry point
│   │       ├── server.rs           # Unix socket IPC server
│   │       ├── config/
│   │       │   └── mod.rs          # Config loading (~/.config/owlry/config.toml)
│   │       ├── data/
│   │       │   ├── mod.rs
│   │       │   └── frecency.rs     # FrecencyStore
│   │       ├── filter.rs           # ProviderFilter, prefix/query parsing
│   │       ├── providers/
│   │       │   ├── mod.rs          # ProviderManager
│   │       │   ├── application.rs  # Desktop application provider
│   │       │   ├── command.rs      # PATH command provider
│   │       │   ├── native_provider.rs  # Bridge: native plugin → provider
│   │       │   └── lua_provider.rs # Bridge: lua/rune runtime → provider
│   │       ├── plugins/
│   │       │   ├── mod.rs
│   │       │   ├── native_loader.rs    # Loads .so plugins
│   │       │   ├── runtime_loader.rs   # Loads lua/rune runtimes
│   │       │   ├── loader.rs
│   │       │   ├── manifest.rs
│   │       │   ├── registry.rs
│   │       │   ├── commands.rs
│   │       │   ├── error.rs
│   │       │   ├── runtime.rs
│   │       │   └── api/            # Lua/Rune scripting API surface
│   │       │       ├── mod.rs
│   │       │       ├── provider.rs
│   │       │       ├── process.rs
│   │       │       ├── cache.rs
│   │       │       ├── math.rs
│   │       │       ├── action.rs
│   │       │       ├── theme.rs
│   │       │       ├── http.rs
│   │       │       ├── hook.rs
│   │       │       └── utils.rs
│   │       ├── notify.rs           # Notification dispatch
│   │       └── paths.rs            # XDG path helpers
│   └── owlry-plugin-api/          # ABI-stable plugin interface (published crate)
│       └── src/
│           └── lib.rs              # PluginVTable, PluginInfo, ProviderInfo, etc.
├── aur/
│   ├── owlry/                     # UI package
│   ├── owlry-core/                # Daemon package (new)
│   ├── owlry-meta-essentials/
│   ├── owlry-meta-full/
│   ├── owlry-meta-tools/
│   └── owlry-meta-widgets/
├── resources/                     # CSS, base themes, icons
├── systemd/
│   ├── owlry-core.service         # Systemd user service
│   └── owlry-core.socket          # Socket activation (optional)
├── justfile
├── README.md
└── docs/
```

### Repo 2 — `owlry-plugins/` (plugins, releases independently)

```
owlry-plugins/
├── Cargo.toml                     # Workspace: all plugins + runtimes
├── crates/
│   ├── owlry-plugin-calculator/
│   ├── owlry-plugin-clipboard/
│   ├── owlry-plugin-emoji/
│   ├── owlry-plugin-bookmarks/
│   ├── owlry-plugin-ssh/
│   ├── owlry-plugin-scripts/
│   ├── owlry-plugin-system/
│   ├── owlry-plugin-websearch/
│   ├── owlry-plugin-filesearch/
│   ├── owlry-plugin-weather/
│   ├── owlry-plugin-media/
│   ├── owlry-plugin-pomodoro/
│   ├── owlry-plugin-systemd/
│   ├── owlry-lua/                 # Lua runtime (cdylib)
│   └── owlry-rune/                # Rune runtime (cdylib)
├── aur/
│   ├── owlry-plugin-*/            # Individual plugin PKGBUILDs
│   ├── owlry-lua/
│   └── owlry-rune/
├── justfile
├── README.md
└── docs/
    ├── PLUGIN_DEVELOPMENT.md
    └── PLUGINS.md
```

---

## 3. Daemon Architecture (`owlry-core`)

### Responsibilities

- Load and manage all native plugins from:
  - `/usr/lib/owlry/plugins/*.so` (system-installed)
  - `~/.local/lib/owlry/plugins/*.so` (user-installed native)
- Load script runtimes from:
  - `/usr/lib/owlry/runtimes/*.so` (system)
  - `~/.config/owlry/plugins/` (user lua/rune scripts)
- Maintain `ProviderManager` with all providers initialized
- Own `FrecencyStore` (persists across UI open/close cycles)
- Load and watch config from `~/.config/owlry/config.toml`
- Listen on Unix socket at `$XDG_RUNTIME_DIR/owlry/owlry.sock`

### Systemd Integration

```ini
# owlry-core.service
[Unit]
Description=Owlry application launcher daemon
Documentation=https://somegit.dev/Owlibou/owlry

[Service]
Type=simple
ExecStart=/usr/bin/owlry-core
Restart=on-failure
Environment=RUST_LOG=warn

[Install]
WantedBy=default.target
```

```ini
# owlry-core.socket (optional socket activation)
[Unit]
Description=Owlry launcher socket

[Socket]
ListenStream=%t/owlry/owlry.sock
DirectoryMode=0700

[Install]
WantedBy=sockets.target
```

Users can either:
- `exec-once owlry-core` in compositor config
- `systemctl --user enable --now owlry-core.service`
- Rely on socket activation (daemon starts on first UI connection)

---

## 4. IPC Protocol

JSON messages over Unix domain socket, newline-delimited (`\n`). Each message is a single JSON object on one line.

### Client → Server

```json
{"type": "query", "text": "fire", "modes": ["app", "cmd"]}
```
Query providers. `modes` is optional — omit to query all. Server streams back results.

```json
{"type": "launch", "item_id": "firefox.desktop", "provider": "app"}
```
Notify daemon that an item was launched (updates frecency).

```json
{"type": "providers"}
```
List all available providers (for UI tab rendering).

```json
{"type": "refresh", "provider": "clipboard"}
```
Force a specific provider to refresh its data.

```json
{"type": "toggle"}
```
Used for toggle behavior — if UI is already open, close it.

```json
{"type": "submenu", "plugin_id": "systemd", "data": "docker.service"}
```
Request submenu items for a plugin.

### Server → Client

```json
{"type": "results", "items": [{"id": "firefox.desktop", "title": "Firefox", "description": "Web Browser", "icon": "firefox", "provider": "app", "score": 95}]}
```

```json
{"type": "providers", "list": [{"id": "app", "name": "Applications", "prefix": ":app", "icon": "application-x-executable", "position": "normal"}]}
```

```json
{"type": "submenu_items", "items": [...]}
```

```json
{"type": "error", "message": "..."}
```

```json
{"type": "ack"}
```

---

## 5. UI Client (`owlry`)

### CLI Interface

```
owlry                          # Open with all providers
owlry -m app,cmd,calc          # Open with specific modes
owlry --profile default        # Open with named profile from config
owlry -m dmenu -p "Pick:"      # dmenu mode (bypasses daemon, reads stdin)
```

**Removed flags:**
- `-p` / `--providers` — removed, was confusing overlap with `-m`

Note: `-p` is repurposed as the dmenu prompt flag (short for `--prompt`), matching dmenu/rofi convention.

### Profiles (in `config.toml`)

```toml
[profiles.default]
modes = ["app", "cmd", "calc"]

[profiles.utils]
modes = ["clip", "emoji", "ssh", "sys"]

[profiles.dev]
modes = ["app", "cmd", "ssh", "file"]
```

Usage: `owlry --profile utils`

### Toggle Behavior

When `owlry` is invoked while already open:
1. Client connects to daemon socket
2. Sends `{"type": "toggle"}`
3. If an instance is already showing, it closes
4. If no instance is showing, opens normally

Implementation: the daemon tracks whether a UI client is currently connected and visible.

### Auto-Start Fallback

If the UI tries to connect to the daemon socket and it doesn't exist:
1. Attempt to start via systemd: `systemctl --user start owlry-core`
2. If systemd activation fails, fork `owlry-core` directly
3. Retry connection with brief backoff (100ms, 200ms, 400ms — 3 attempts max)
4. If still unreachable, exit with clear error message

### dmenu Mode

Completely self-contained in the UI binary:
- Reads items from stdin
- Renders GTK picker
- Prints selected item to stdout
- No daemon connection, no frecency, no plugins
- `-p "prompt"` sets the prompt text

---

## 6. Plugin API Decoupling

### `owlry-plugin-api` as Published Crate

- Published to crates.io (or referenced as git dep from core repo)
- Versioned independently — bumped only when ABI changes
- Current `API_VERSION = 3` continues
- Third-party plugin authors: `cargo add owlry-plugin-api`, implement `owlry_plugin_vtable`, build as cdylib

### Plugin Dependencies in `owlry-plugins` Repo

Each plugin's `Cargo.toml`:
```toml
[dependencies]
owlry-plugin-api = "0.5"  # from crates.io
# or
owlry-plugin-api = { git = "https://somegit.dev/Owlibou/owlry.git", tag = "plugin-api-v0.5.0" }
```

No path dependencies between repos.

### Plugin Discovery (daemon)

Auto-discovers all `.so` files in plugin directories. No explicit enable list needed.

Disabling in `config.toml`:
```toml
[plugins]
disabled = ["pomodoro", "media"]
```

---

## 7. Versioning Strategy

### Core Repo (`owlry`)

| Crate | Versioning | When to bump |
|-------|-----------|-------------|
| `owlry-plugin-api` | Independent semver | ABI changes only (rare) |
| `owlry-core` | Independent semver | Daemon logic, IPC, provider changes |
| `owlry` | Independent semver | UI changes, CLI changes |

These can share a workspace version if convenient, but are not required to stay in lockstep.

### Plugins Repo (`owlry-plugins`)

| Crate | Versioning | When to bump |
|-------|-----------|-------------|
| Each plugin | Independent semver | That plugin's code changes |
| `owlry-lua` | Independent semver | Lua runtime changes |
| `owlry-rune` | Independent semver | Rune runtime changes |

A calculator bugfix bumps only `owlry-plugin-calculator`. Nothing else changes.

### AUR Packages

- Core PKGBUILDs source from `owlry` repo tags
- Plugin PKGBUILDs source from `owlry-plugins` repo, each with their own tag
- Meta-packages stay at `1.0.0`, only `pkgrel` changes when included packages update

---

## 8. Dependency Refresh

Before starting implementation, update all external crate dependencies to latest stable versions:
- Run `cargo update` to refresh `Cargo.lock`
- Review and bump version constraints in `Cargo.toml` files where appropriate (e.g., `gtk4`, `abi_stable`, `reqwest`, `mlua`, `rune`, etc.)
- Verify the workspace still builds and tests pass after updates

---

## 9. Migration Phases

### Phase 0 — Dependency Refresh
- Update all external crates to latest stable versions
- Verify build + tests pass
- Commit

### Phase 1 — Extract `owlry-core` Crate (library, no IPC yet)
- Create `crates/owlry-core/` with its own `Cargo.toml`
- Move from `crates/owlry/src/` into `crates/owlry-core/src/`:
  - `config/` (config loading)
  - `data/` (frecency)
  - `filter.rs` (provider filter, prefix parsing)
  - `providers/` — `mod.rs`, `application.rs`, `command.rs`, `native_provider.rs`, `lua_provider.rs`
  - `plugins/` — entire directory (native_loader, runtime_loader, manifest, registry, commands, error, runtime, api/)
  - `notify.rs`
  - `paths.rs`
- `owlry` depends on `owlry-core` as a library (path dep)
- Launcher still works as before — no behavioral change
- Update justfile for new crate
- Update README to reflect new structure
- Commit

### Phase 2 — Add IPC Layer (daemon + client)
- Add `server.rs` to `owlry-core` — Unix socket listener, JSON protocol
- Add `main.rs` to `owlry-core` — daemon entry point
- Add `[[bin]]` target to `owlry-core` alongside the library (`lib` + `bin` in same crate)
- Add `client.rs` to `owlry` — connects to daemon, sends queries
- Wire up `owlry` UI to use IPC client instead of direct library calls
- Implement toggle behavior
- Implement auto-start fallback
- Implement profiles in config
- Add systemd service + socket files
- dmenu mode stays self-contained in `owlry`
- Remove `-p` provider flag, repurpose as `--prompt` for dmenu
- Update justfile
- Update README
- Commit

### Phase 3 — Split Repos
- Create `owlry-plugins/` directory with its own workspace
- Move all `crates/owlry-plugin-*/` into `owlry-plugins/crates/`
- Move `crates/owlry-lua/` and `crates/owlry-rune/` into `owlry-plugins/crates/`
- Update plugin `Cargo.toml` files: path dep → git/crates.io dep for `owlry-plugin-api`
- Move plugin AUR PKGBUILDs into `owlry-plugins/aur/`
- Move runtime AUR PKGBUILDs into `owlry-plugins/aur/`
- Create `owlry-plugins/justfile`
- Move `docs/PLUGIN_DEVELOPMENT.md` and `docs/PLUGINS.md` into `owlry-plugins/docs/`
- Create `owlry-plugins/README.md`
- Update core `owlry/justfile` (remove plugin-related targets)
- Update core `owlry/README.md`
- Add `owlry-core` AUR PKGBUILD
- Update meta-package PKGBUILDs to reflect new source structure
- Update `CLAUDE.md`
- Commit both repos

### Phase 4 — Polish & Verify
- Verify all AUR PKGBUILDs build correctly
- Verify `owlry-core` daemon starts and responds to IPC
- Verify `owlry` UI connects, queries, renders, and launches
- Verify dmenu mode still works standalone
- Verify toggle behavior
- Verify profile loading
- Verify auto-start fallback
- Verify systemd service and socket activation
- Verify plugin loading from both system and user paths
- Clean up any leftover references to old structure
- Final README review

---

## 10. Out of Scope

- Publishing to crates.io (can be done later)
- Pushing updated PKGBUILDs to AUR git repos (done after verification)
- Alternative frontends (TUI, etc.) — the architecture enables this but it's future work
- Plugin hot-reloading in the daemon
- Multi-client support (only one UI at a time for now)
