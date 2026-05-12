# Owlry v2 Restructure Plan

**Status:** in progress on branch `v2` (cut from `main @ 1caa050`)
**Target version:** `owlry 2.0.0`
**Scope:** repository collapse, C-ABI removal, runtime consolidation, CLI cleanup, single AUR package
**Originating issue:** [#5 uuctl launcher not working](https://somegit.dev/Owlibou/owlry/issues/5) — closed by deletion of the C-ABI loader that caused it

---

## 0. Decisions log

All design decisions agreed before work started. These are load-bearing — do not silently change them.

| # | Decision | Rationale |
|---|---|---|
| D1 | Collapse to **1 AUR package**: `owlry` | 14 PKGBUILDs maintained in lockstep was the root cost of every API bump. Native plugin distribution was theoretical (no third-party shipped any). |
| D2 | **Drop the C-ABI plugin system entirely** | Issue #5's root cause: strict `api_version != API_VERSION` rejection. Compiling providers in eliminates the failure mode. |
| D3 | **Drop Rune runtime** | Only one user plugin (`hyprshutdown`) uses Rune; will be ported to Lua. 118 MB dylib for one plugin is not earned. |
| D4 | **Use Lua for config + plugins** via `mlua` (Lua 5.4, bundled) — *Phase 3+* | Single language for both configuration and user extension. LuaJIT perf is irrelevant for a config language. |
| D5 | **Configs-as-plugins** model | `owlry.provider {}` in `init.lua` registers a runtime provider treated identically to a built-in. No separate `~/.config/owlry/plugins/` dir. |
| D6 | **Daemon as subcommand** with `-d` short alias: `owlry daemon` or `owlry -d` | Single binary; `owlryd` no longer exists. systemd unit invokes `owlry -d`. |
| D7 | **`-m auto`** as explicit alias for the implicit no-flag default | Today's `accept_all` behavior — all enabled providers active, tabs cycle. Pinned with an integration test so refactors can't regress it. |
| D8 | **Hard cut on TOML config** in a later phase | Migrator command `owlry migrate-config` reads existing TOML + script plugins, emits `init.lua`. One-shot; TOML reader deleted in the same release that ships Lua. |
| D9 | **AUR ships `--features full`** | Defaults in `Cargo.toml` are minimal; AUR PKGBUILD enables everything. End-user picks at runtime via config, not at install time. |
| D10 | **Runtime activation is config-driven** | Compiled-in ≠ enabled. Config (TOML now, Lua later) decides which providers run. |
| D11 | **Drop `config_editor` provider** (1127 LOC) | An in-launcher settings editor is replaced by editing `init.lua` + `owlry config validate`. |
| D12 | **Drop `scripts` provider** | Replaced by `owlry.provider {}` in Lua config (Phase 3+). |
| D13 | **Rename `sys` → `power`** | "sys" collides mentally with "systemd". `:sys` kept as alias. |
| D14 | **Keep submenu protocol** | Used by systemd provider for service actions. Becomes a method on the `Provider` trait. |
| D15 | **Rename systemd unit `owlryd.{service,socket}` → `owlry.{service,socket}`** | Consistent with single-binary collapse. PKGBUILD ships new names; old units cleaned up via `replaces=()` upgrade path. |
| D16 | **Submenu protocol redesign deferred to Phase 5** | Today's `SUBMENU:type:data` string encoding works. Typed IPC variant is hygiene, not blocking. |
| D17 | **Keep the daemon** | Cold-start cost (XDG scan, frecency load, provider warm-up) is the real reason the daemon exists. Single-process collapse remains an option to revisit post-2.0 if profiling proves it's free. |
| D18 | **TOML reader: hard cut at 3.0.0** | One migration moment. 2.x stays TOML-only; 3.0 ships Lua-only with `owlry migrate-config` shipping in 2.x as preview. |
| D19 | **Lua sandbox** (Phase 3): filesystem read + process spawn allowed; no network by default | Launcher needs to read configs and spawn commands. Network access (HTTP, sockets) requires explicit config opt-in. |
| D20 | **Widget providers (`weather`, `media`, `pomodoro`) on hold** | UI positioning unresolved. **Excluded from Phase 1 conversion entirely** — not pulled in, not compiled in. Revisit as a dedicated workstream after 2.0 ships. |
| D21 | **Hot-reload of `init.lua` on save** (Phase 3) | `notify` crate already a dep. Watch `~/.config/owlry/init.lua` and any `require`d files; re-run config eval on change. |

---

## 1. Target shape (post-Phase 2 — what 2.0.0 ships as)

### Repo layout

```
owlry/
├── Cargo.toml                 -- workspace with single member
├── crates/owlry/
│   ├── Cargo.toml             -- one binary, features per provider
│   └── src/
│       ├── main.rs            -- subcommand router
│       ├── cli.rs             -- clap defs
│       ├── server.rs          -- daemon mode (was owlry-core/src/server.rs)
│       ├── client.rs          -- IPC client to daemon (UI mode)
│       ├── backend.rs         -- abstracts daemon vs local
│       ├── ipc.rs             -- Request/Response types
│       ├── filter.rs          -- ProviderFilter
│       ├── paths.rs           -- XDG paths
│       ├── notify.rs          -- desktop notifications
│       ├── theme.rs           -- CSS loading
│       ├── config/            -- config loader (TOML in Phase 1, Lua in Phase 3)
│       ├── data/              -- FrecencyStore
│       ├── providers/         -- ALL providers, each gated by feature
│       │   ├── mod.rs         -- ProviderManager, Provider traits, ProviderType
│       │   ├── application.rs (feature: app)
│       │   ├── command.rs     (feature: cmd)
│       │   ├── calculator.rs  (feature: calc)
│       │   ├── converter/     (feature: conv)
│       │   ├── power.rs       (feature: power, was sys)
│       │   ├── dmenu.rs       (feature: dmenu, moved from owlry/src/providers/)
│       │   ├── bookmarks.rs   (feature: bookmarks)
│       │   ├── clipboard.rs   (feature: clipboard)
│       │   ├── emoji.rs       (feature: emoji)
│       │   ├── ssh.rs         (feature: ssh)
│       │   ├── systemd.rs     (feature: systemd, type_id "uuctl")
│       │   ├── websearch.rs   (feature: websearch)
│       │   └── filesearch.rs  (feature: filesearch)
│       │   -- weather, media, pomodoro: deferred (D20)
│       └── ui/                -- GTK4 client
├── data/
│   ├── config.example.toml    -- shipped defaults (Phase 1)
│   └── default-config.lua     -- shipped defaults (Phase 3+)
├── systemd/
│   ├── owlryd.service         -- renamed to owlry.service in Phase 2; ExecStart=/usr/bin/owlry -d
│   └── owlryd.socket          -- renamed to owlry.socket
└── aur/owlry/PKGBUILD         -- the only AUR package
```

### Deleted entirely

| Path | Reason |
|---|---|
| `crates/owlry-core/` | Folded into `crates/owlry/` |
| `crates/owlry-plugin-api/` | C-ABI gone |
| `crates/owlry-lua/` | mlua compiled in via Phase 3 |
| `crates/owlry-rune/` | Rune dropped (D3) |
| `crates/owlry-core/src/plugins/` (entire dir) | C-ABI loader, runtime loader, manifest, registry, watcher, error |
| `crates/owlry-core/src/providers/native_provider.rs` | C-ABI bridge |
| `crates/owlry-core/src/providers/lua_provider.rs` | Old Lua bridge |
| `crates/owlry-core/src/providers/config_editor.rs` | D11 |
| `crates/owlry/src/plugin_commands.rs` (1296 LOC) | No plugin CLI |
| `aur/owlry-core/`, `aur/owlry-lua/`, `aur/owlry-rune/` | Folded into `aur/owlry/` |
| `aur/owlry-plugin-*` (none yet in this repo) | n/a |
| `aur/owlry-meta-*` (essentials, widgets, tools, full) | Redundant |
| `owlry-plugins/` sibling repo | Archived on somegit.dev |

---

## 2. CLI shape (post-Phase 1)

```
owlry                              launch UI, auto mode (default)
owlry -m auto                      launch UI, auto mode (explicit alias)
owlry -m <mode>                    launch UI in single-provider mode
owlry --profile <name>             launch UI with a named profile

owlry daemon                       run daemon (alias: owlry -d)
owlry dmenu [-p <prompt>]          dmenu mode (was: owlry -m dmenu)
owlry doctor                       diagnostics: providers, config, socket, runtime
owlry providers [<name>]           list providers (or show one)
owlry config validate              check config for errors
owlry config show                  print resolved effective config
owlry migrate-config               TOML → init.lua (Phase 3+)
```

### Removed CLI surface

The entire `owlry plugin ...` subcommand tree (~1296 LOC in `plugin_commands.rs`) is deleted:
`list`, `search`, `info`, `install`, `remove`, `update`, `enable`, `disable`, `create`, `validate`, `runtimes`, `run`, `commands`.
There is no plugin installation to manage — features are compiled in.

---

## 3. Feature names + descriptions

Final naming for cargo features and corresponding provider IDs:

| Feature      | CLI mode / prefix     | What it does |
|---|---|---|
| `app`        | `:app`                | XDG desktop applications |
| `cmd`        | `:cmd`                | Executables on `$PATH` |
| `calc`       | `:calc` / `=`         | Calculator (math expressions, e.g. `= 2+2`) |
| `conv`       | `:conv`               | Unit & currency converter (e.g. `5 ft to m`) |
| `power`      | `:power` (`:sys` alias) | Shutdown, reboot, logout, suspend, lock — was `sys` |
| `dmenu`      | (subcommand)          | Pipe-based selection — stdin in, selection out |
| `bookmarks`  | `:bm`                 | Browser bookmarks (Firefox, Chromium) |
| `clipboard`  | `:clip`               | Clipboard history via `cliphist`/`wl-clipboard` |
| `emoji`      | `:emoji`              | Emoji picker (writes via `wl-clipboard`) |
| `ssh`        | `:ssh`                | SSH hosts from `~/.ssh/config` |
| `systemd`    | `:systemd` (`:uuctl` alias) | systemd user units (type_id stays "uuctl" for config compat) |
| `websearch`  | `:web` / `?`          | Web search (DDG, Google, custom engines) |
| `filesearch` | `:file` / `/`         | File search via `fd` or `mlocate` |
| ~~`weather`~~ | — | **Deferred (D20)** — not in 2.0.0 |
| ~~`media`~~  | — | **Deferred (D20)** — not in 2.0.0 |
| ~~`pomodoro`~~ | — | **Deferred (D20)** — not in 2.0.0 |

### Cargo feature groups

```toml
[features]
default = ["app", "cmd", "calc", "conv", "power", "dmenu"]
full = [
    "app", "cmd", "calc", "conv", "power", "dmenu",
    "bookmarks", "clipboard", "emoji", "ssh", "systemd",
    "websearch", "filesearch",
    # widgets (weather, media, pomodoro): deferred (D20)
]
dev-logging = []
```

AUR PKGBUILD builds with `--features full`.

---

## 4. Runtime config (Phase 1: TOML, Phase 3+: Lua)

Three orthogonal axes:

1. **Compiled in** — Cargo features at build time. AUR ships `full`.
2. **Enabled** — Config decides which providers actually run.
3. **Selected** — `-m` / `--profile` / Tab key narrows the active set at use time.

### Phase 1 (TOML, kept temporarily)

```toml
[general]
tabs = ["app", "cmd", "calc", "conv", "power"]   # what shows in auto mode

[providers]
applications = true
commands = true
calculator = true
converter = true
power = true            # was `system`
bookmarks = false       # opt-in
clipboard = false
# ...etc
```

### Phase 3+ (Lua, target)

```lua
local owlry = require("owlry")

owlry.set {
  theme = "apex-neon",
  width = 850, height = 650,
  tabs = { "app", "cmd", "power" },
}

owlry.providers { "app", "cmd", "calc", "conv", "power", "bookmarks", "systemd" }

owlry.provider {
  id = "hs", prefix = ":hs", tab_label = "Shutdown",
  items = function(_q)
    return {
      { name = "Lock",     command = "hyprlock" },
      { name = "Shutdown", command = "systemctl poweroff" },
      { name = "Reboot",   command = "systemctl reboot" },
    }
  end,
}
```

---

## 5. Phased plan

### Phase 1 — Repo collapse (current)

Goal: single workspace member, single binary, TOML config still works, C-ABI gone, all built-in providers compiled in behind features.

Subtasks (tracked in TaskList):

1. ✅ Inventory: map Provider trait + plugin shapes
2. **Workspace skeleton: collapse to single crate** — move `owlry-core/src/*` into `owlry/src/`, absorb deps, remove other crates from workspace
3. **Delete C-ABI plugin system** — `plugins/` dir, `native_provider.rs`, `lua_provider.rs`, `owlry-plugin-api` crate; strip `ProviderManager::new_with_config()` of plugin loading
4. **Delete Rune + Lua runtime crates** — `crates/owlry-rune/`, `crates/owlry-lua/`
5. **Delete config_editor + scripts providers**
6. **Convert 8 plugins to native Provider impls** — bookmarks, clipboard, emoji, ssh, systemd, websearch, filesearch. Pull source from `owlry-plugins/crates/owlry-plugin-*`, convert `extern "C"` vtable → `impl Provider` / `impl DynamicProvider`. Per-plugin mechanical work. **Widgets (weather, media, pomodoro) excluded per D20; scripts excluded per D12.**
7. **Wire cargo features per provider** — `#[cfg(feature = "...")]` gating; `default` / `full` feature groups
8. **Rename `sys` → `power`** — file, type_id (in CLI mode mapping table), `:sys` kept as alias, config key `providers.system` → `providers.power` (with TOML migration shim that reads the old name)
9. **CLI restructure** — new clap shape (subcommands `daemon`, `dmenu`, `doctor`, `providers`, `config`, `migrate-config`); drop entire `plugin` subcommand tree; daemon mode via `owlry -d` / `owlry daemon`
10. **Auto-mode integration test** — `tests/auto_mode.rs` asserts the no-flag default still surfaces results from all enabled providers and tabs match `general.tabs`
11. **Phase 1 final build + smoke** — `cargo check --workspace`, `cargo build --release`, `cargo test`, runtime socket smoke

### Phase 2 — AUR republish as 2.0.0

Goal: ship the collapsed binary to users. Issue #5 closes by virtue of the C-ABI being gone.

Steps:

- Update `aur/owlry/PKGBUILD`:
  - `pkgver=2.0.0`
  - `replaces=('owlry-core' 'owlry-lua' 'owlry-rune' 'owlry-plugin-bookmarks' 'owlry-plugin-clipboard' 'owlry-plugin-emoji' 'owlry-plugin-filesearch' 'owlry-plugin-media' 'owlry-plugin-pomodoro' 'owlry-plugin-scripts' 'owlry-plugin-ssh' 'owlry-plugin-systemd' 'owlry-plugin-weather' 'owlry-plugin-websearch' 'owlry-meta-essentials' 'owlry-meta-widgets' 'owlry-meta-tools' 'owlry-meta-full')`
  - `conflicts=(<same list>)`
  - `provides=(<same list>)`
  - `build()` uses `cargo build --release --features full`
  - `package()` installs single binary, single systemd service+socket, default config
- Delete the 14 other PKGBUILDs under `aur/`
- `git tag owlry-v2.0.0`
- Push, build, `aur-publish`
- Archive `owlry-plugins` sibling repo on somegit.dev (read-only, history preserved)
- Comment on issue #5: "fixed by 2.0 rewrite; the C-ABI plugin loader that produced this class of bug has been removed"

### Phase 3 — Lua config core (later release: 2.1.0 or 3.0.0)

Goal: replace TOML with Lua-driven config. Hyprland-style configs-as-code.

Steps:

- Add `mlua = { version = "0.11", features = ["lua54", "vendored", "send", "serialize"] }` dependency
- Build the `owlry.*` Lua API surface:
  - `owlry.set { ... }` — global settings (theme, dimensions, tabs)
  - `owlry.providers { ... }` — enable list (subset of compiled-in features)
  - `owlry.provider { id, prefix, tab_label, items, ... }` — register a runtime provider
  - `owlry.bind(key, action)` — keybinding overrides
  - `owlry.theme(name | { ... })` — theme selection or inline definition
- Resolve config: `~/.config/owlry/init.lua` if present; else fall back to shipped `data/default-config.lua`
- `owlry migrate-config` subcommand: read existing `config.toml` (+ `~/.config/owlry/plugins/*` if any) and emit equivalent `init.lua`
- Wire `owlry config validate` and `owlry config show` to surface Lua errors and resolved state
- Decision pending: hard cut TOML in 3.0.0 vs. dual-read in 2.1.0 → cut in 3.0.0

### Phase 4 — Configs-as-plugins (same release as Phase 3)

Goal: Lua-registered providers are first-class. Drop the script plugin discovery dir.

Steps:

- `owlry.provider {}` creates a `LuaProvider` that wraps the supplied Lua closures and implements the `Provider` trait
- Remove the `~/.config/owlry/plugins/` filesystem discovery entirely
- The shipped `default-config.lua` showcases at least one inline provider as documentation

### Phase 5 — Hygiene

Goal: clean up debt that the collapse uncovered.

Steps:

- Update `CLAUDE.md` — binary name (`owlry`, not `owlry-core`/`owlryd`), workspace layout, drop "Phase 5 pending" wording
- Update `README.md` — new CLI, new install instructions, drop AUR package list
- Refresh `ROADMAP.md`
- Split files >1000 LOC:
  - `providers/mod.rs` — separate `ProviderType` / `LaunchItem` / `ItemSource` from `ProviderManager`
  - `ui/main_window.rs` — input handling, render, signal wiring as separate modules
  - `providers/converter/units.rs` — move data tables to a `units.toml` resource
- Fix double-daemon spawn: rely on socket activation only; add `flock` guard in `main.rs` if needed
- Make sure systemd unit name is `owlry.service` (or `owlryd.service`?) — decide before AUR ship

---

## 6. Provider-by-provider conversion notes (Phase 1.6)

Mechanical pattern for each plugin: drop `extern "C"` exports, `PluginItem` → `LaunchItem`, opaque `ProviderHandle` → struct field on a `Provider` impl.

### `provider_kind`-driven categorization (current state)

| Plugin           | Kind     | Submenu? | Per-keystroke? | Notes |
|---|---|---|---|---|
| bookmarks        | Static   | no       | no             | SQLite for Firefox; bundled rusqlite |
| clipboard        | Static   | yes (?)  | no             | Uses `cliphist`, `wl-clipboard` |
| emoji            | Static   | no       | no             | Bundled emoji data; writes via `wl-clipboard` |
| ssh              | Static   | no       | no             | Parses `~/.ssh/config` |
| systemd          | Static   | **yes**  | no             | type_id "uuctl"; SUBMENU: protocol for service actions |
| scripts          | DELETE   | —        | —              | Replaced by Lua config (D12) |
| websearch        | Dynamic  | no       | yes            | per-keystroke; `?` trigger |
| filesearch       | Dynamic  | no       | yes            | per-keystroke; `/` trigger; `fd` or `mlocate` |
| ~~weather~~      | DEFER    | —        | —              | **D20** — UI positioning unresolved |
| ~~media~~        | DEFER    | —        | —              | **D20** — UI positioning unresolved |
| ~~pomodoro~~     | DEFER    | —        | —              | **D20** — UI positioning unresolved |

### Submenu mechanism on `Provider` trait

Add to `pub trait Provider`:

```rust
/// Returns submenu actions for the given encoded data string, or empty if none.
/// Called when a user selects an item whose command field starts with "SUBMENU:".
fn submenu_actions(&self, _data: &str) -> Vec<LaunchItem> { Vec::new() }

/// Handle a plugin-defined action command (e.g. "POMODORO:start").
/// Returns true if handled.
fn execute_action(&self, _command: &str) -> bool { false }

/// Optional UI hints — replace today's hardcoded match table.
fn prefix(&self) -> Option<&str> { None }
fn icon(&self) -> Option<&str> { None }
fn position(&self) -> ProviderPosition { ProviderPosition::Normal }
fn priority(&self) -> u32 { 0 }
```

`ProviderManager::query_submenu_actions` and `execute_plugin_action` route to these trait methods instead of the C-ABI `NativeProvider`.

---

## 7. Breaking changes / migration impact

### What users lose at 2.0.0

- 14 separate AUR packages → 1 (paru handles via `replaces`/`conflicts`)
- Ability to install a subset of plugins via AUR (still possible via `cargo install --no-default-features --features ...`)
- Rune scripting (the user's `hyprshutdown` plugin breaks; will be ported to Lua in Phase 3, or compiled into a small custom build)
- Dynamic plugin loading at runtime (was never used by anyone outside Owlibou)
- `~/.config/owlry/plugins/` directory (Phase 4 — but already orphaned at 2.0.0 since Rune is gone)

### What stays unchanged for users

- Launcher behavior, keybindings, themes, search prefixes
- Mode flags: `-m app`, `-m systemd`, `-m uuctl` (alias), `-m power` (was `-m sys`)
- TOML config compatibility through 2.x — until Phase 3 hard-cut to Lua
- Socket protocol (mostly — `Request::PluginList` is dropped; `Submenu` and `PluginAction` stay)
- systemd integration (unit file invokes `owlry -d` instead of `owlryd`)

### `replaces=()` array for `aur/owlry/PKGBUILD` (2.0.0)

```
owlry-core
owlry-lua
owlry-rune
owlry-plugin-bookmarks
owlry-plugin-clipboard
owlry-plugin-emoji
owlry-plugin-filesearch
owlry-plugin-media
owlry-plugin-pomodoro
owlry-plugin-scripts
owlry-plugin-ssh
owlry-plugin-systemd
owlry-plugin-weather
owlry-plugin-websearch
owlry-meta-essentials
owlry-meta-widgets
owlry-meta-tools
owlry-meta-full
```

---

## 8. Open questions — RESOLVED

All section-8 questions from the original plan have been answered. Captured as D15–D21 in section 0.

| Original question | Resolution | Recorded as |
|---|---|---|
| systemd unit name | Rename to `owlry.{service,socket}` | D15 |
| Submenu protocol redesign | Deferred to Phase 5 | D16 |
| Drop daemon entirely? | No — cold-start cost justifies it | D17 |
| TOML reader removal timing | Hard cut at 3.0.0 | D18 |
| Lua sandbox model | fs read + process spawn allowed; no network by default | D19 |
| Widget positioning | On hold; widgets excluded from Phase 1 | D20 |
| Hot-reload init.lua | Yes (Phase 3) | D21 |

### Newly open (post-resolution)

| Question | Defer to | Notes |
|---|---|---|
| Widget rework: keep the "Widget position" UI concept, redesign, or drop entirely? | Post-2.0 workstream | Pulled out of Phase 1 scope per D20. Decide before re-introducing weather/media/pomodoro. |
| `owlry migrate-config` shipped in 2.x as preview, or only at 3.0.0? | Phase 3 planning | Lean toward shipping in 2.1.0 as a preview so users can test their migrations before TOML is gone. |

---

## 9. Build / verify checklist (Phase 1 acceptance)

Phase 1 is "done" when all of these pass:

- [ ] `cargo check --workspace` (no warnings on default features)
- [ ] `cargo check --workspace --all-features` (no warnings)
- [ ] `cargo build --release --features full`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo fmt --all --check`
- [ ] Workspace contains only `crates/owlry` member
- [ ] `crates/owlry-core/`, `crates/owlry-plugin-api/`, `crates/owlry-lua/`, `crates/owlry-rune/` no longer exist
- [ ] No occurrences of `owlry_plugin_api`, `mlua`, `libloading`, `Rune`, `liblua.so`, `librune.so` outside historical docs
- [ ] `target/release/owlry` runs:
  - `owlry --help` shows new subcommand structure
  - `owlry doctor` reports providers + config + socket status
  - `owlry -d` starts daemon; socket listens at `$XDG_RUNTIME_DIR/owlry/owlry.sock`
  - `owlry -m systemd` shows systemd units (Issue #5 fixed locally)
  - `owlry` (no flags) shows auto mode with results from all enabled providers, tabs match `general.tabs`
- [ ] Integration test `tests/auto_mode.rs` passes
- [ ] No daemon double-spawn (verify `pgrep -af owlry` shows one process when launched normally)

---

## 10. Working notes (mutable)

This section captures in-progress state. Update freely as work proceeds.

- **Branch:** `v2`, cut from `main @ 1caa050` on 2026-05-13
- **Current task:** #3 (Delete C-ABI plugin system) — in progress
- **Stray processes from inventory phase:**
  - PIDs 3042, 3278 — pre-existing `owlryd` (double-spawn bug; will resolve via Phase 5)
  - PID 594897 — test daemon from inventory probe; harness denied kill; resolves at next reboot or user kill
- **Hyprland exec-once for owlryd:** suspected source of double-spawn; verify and remove during Phase 5
- **`plugin-api-v1.0.1` git tag:** still has `API_VERSION = 3`. Moot once C-ABI is deleted.
