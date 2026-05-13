# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A lightweight, owl-themed application launcher for Wayland, built with GTK4 and Layer Shell. Single-binary, configurable, fast.

> **2.0 highlights.** Owlry collapsed from 15 AUR packages and a dynamic plugin system into one binary. All providers (apps, commands, calculator, converter, power, bookmarks, clipboard, emoji, ssh, systemd, websearch, filesearch) are compiled in and gated by cargo features. The AUR build ships everything; `cargo install` consumers can pick a subset. See [`docs/RESTRUCTURE-V2.md`](docs/RESTRUCTURE-V2.md) for the full rewrite story.

## Features

- **Single binary** — UI client, daemon, and providers in one `/usr/bin/owlry`
- **Client/daemon architecture** — Daemon (`owlry -d`) keeps providers warm; UI appears instantly
- **Built-in providers** — Apps, PATH commands, calculator, unit/currency converter, power actions
- **Optional providers** (compiled in via `--features full` on AUR) — Clipboard history, emoji, SSH hosts, systemd user units, web search, filesystem search
- **Fuzzy search with tags** — Fast matching across names, descriptions, category tags
- **Config profiles** — Named mode presets for different workflows
- **Filter prefixes** — Scope searches with `:app`, `:cmd`, `:power`, `:uuctl`, `:tag:X`, etc.
- **Frecency ranking** — Frequently/recently used items rank higher
- **Toggle behavior** — Bind one key to open/close the launcher
- **GTK4 theming** — System theme by default, 10 built-in themes shipped
- **Wayland native** — Uses Layer Shell for proper overlay behavior
- **dmenu compatible** — Pipe-based selection, no daemon required

## Installation

### Arch Linux (AUR)

```bash
paru -S owlry         # or yay -S owlry
```

Upgrading from 1.x: paru/pacman transparently swaps the old `owlry-core`, `owlry-lua`, `owlry-rune`, and every `owlry-plugin-*` and `owlry-meta-*` package for the new unified `owlry`. The `.install` hook prints a banner with the systemd unit rename instructions (`owlryd.{service,socket}` → `owlry.{service,socket}`).

### Build from Source

**System dependencies:**

```bash
# Arch
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu / Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

**Rust 1.90+:**

```bash
git clone https://somegit.dev/Owlibou/owlry.git
cd owlry
cargo build --release --features full     # full = all providers (matches the AUR build)
just install-local                        # installs binary + systemd units (sudo)
```

Cargo features (pick a subset if you don't need everything):

| Feature       | Provider                                  | Default? |
|---|---|---|
| `app`         | XDG desktop applications                  | yes |
| `cmd`         | Executables on `$PATH`                    | yes |
| `calc`        | Calculator                                | yes |
| `conv`        | Unit & currency converter                 | yes |
| `power`       | Shutdown/reboot/lock                      | yes |
| `dmenu`       | Pipe-based selection                      | yes |
| `clipboard`   | Clipboard history (`cliphist`)            | opt-in |
| `emoji`       | Emoji picker (`wl-clipboard`)             | opt-in |
| `ssh`         | SSH hosts from `~/.ssh/config`            | opt-in |
| `systemd`     | systemd user units (type_id: `uuctl`)     | opt-in |
| `websearch`   | Web search (DuckDuckGo / configurable)    | opt-in |
| `filesearch`  | `fd` / `mlocate` shellout                 | opt-in |
| `full`        | All of the above                          | — |

```bash
cargo build --release --no-default-features --features "app,cmd,calc,conv,power,dmenu,systemd"
```

## Getting Started

### Starting the Daemon

Three options:

**1. Systemd user service (recommended)**

```bash
systemctl --user enable --now owlry.service
```

Reload config from disk without restarting:

```bash
systemctl --user reload owlry.service     # or: kill -HUP $(pidof owlry)
```

**2. Socket activation**

```bash
systemctl --user enable owlry.socket
```

Daemon starts the first time the UI connects.

**3. Compositor autostart**

```
# Hyprland
exec-once = owlry -d

# Sway
exec owlry -d
```

### Launching the UI

```bash
# Hyprland
bind = SUPER, Space, exec, owlry

# Sway
bindsym $mod+space exec owlry
```

Running `owlry` while a window is already open sends a toggle command — single keybind acts as open/close. If the daemon isn't running, the UI tries to start it via systemd.

## Usage

```
owlry                          launch UI, auto mode
owlry -m auto                  launch UI, auto mode (explicit alias)
owlry -m <mode>                launch UI in single-provider mode
owlry --profile <name>         launch UI with a named profile

owlry -d                       run the daemon (alias: `owlry daemon`)
owlry dmenu [-p <prompt>]      dmenu mode (reads stdin, prints selection)
owlry doctor                   diagnostics: config + socket + providers
owlry providers [<id>]         list providers (or show details for one)
owlry config validate          parse config, report errors (exit 1) and warnings (exit 2)
owlry config show              print the resolved effective config as TOML
owlry migrate-config [--force] TOML → owlry.lua (deterministic; refuses to overwrite without --force)
```

### Profiles

```toml
[profiles.dev]
modes = ["app", "cmd", "ssh"]

[profiles.media]
modes = ["emoji", "clipboard"]
```

```bash
owlry --profile dev
```

### dmenu Mode

`owlry dmenu` (or the legacy `owlry -m dmenu`) reads stdin and prints the selection to stdout. It runs locally — no daemon required.

```bash
# Screenshot menu
printf '%s\n' \
  "grimblast --notify copy screen" \
  "grimblast --notify copy area" \
  "grimblast --notify edit screen" \
  | owlry dmenu -p "Screenshot" \
  | sh

# Git branch checkout
git branch | owlry dmenu -p "checkout" | xargs git checkout

# Kill a process
ps -eo comm | sort -u | owlry dmenu -p "kill" | xargs pkill

# Open a project
find ~/projects -maxdepth 1 -type d | owlry dmenu | xargs code

# Package manager
pacman -Ssq | owlry dmenu -p "install" | xargs sudo pacman -S
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Launch selected item |
| `Escape` | Close launcher / exit submenu |
| `Up` / `Down` | Navigate results |
| `Tab` | Cycle filter tabs |
| `Shift+Tab` | Cycle filter tabs (reverse) |
| `Ctrl+1..9` | Toggle tab by position |

### Search Prefixes

| Prefix | Provider | Example |
|--------|----------|---------|
| `:app` | Applications | `:app firefox` |
| `:cmd` | PATH commands | `:cmd git` |
| `:power` (`:sys`, `:system`) | Power & session actions | `:power shutdown` |
| `:calc` | Calculator | `:calc sqrt(16)` |
| `:conv` | Converter | `:conv 5 ft to m` |
| `:clip` | Clipboard | `:clip password` |
| `:emoji` | Emoji | `:emoji heart` |
| `:ssh` | SSH hosts | `:ssh server` |
| `:uuctl` (`:systemd`) | systemd user units | `:uuctl dbus` |
| `:web` | Web search | `:web rust docs` |
| `:file` | Files | `:file config` |
| `:tag:X` | Filter all results by tag | `:tag:development` |

### Trigger Prefixes

| Trigger | Provider | Example |
|---------|----------|---------|
| `=` | Calculator | `= 5+3` |
| `>` | Converter | `> 20 km to mi` |
| `?` | Web search | `? rust programming` |
| `/` | File search | `/ .bashrc` |

## Configuration

Owlry follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/):

| Path | Purpose |
|------|---------|
| `~/.config/owlry/owlry.lua` | Lua configuration (preferred, takes precedence over `config.toml`) |
| `~/.config/owlry/config.toml` | TOML configuration (legacy; ignored when `owlry.lua` exists) |
| `~/.config/owlry/themes/*.css` | Custom themes |
| `~/.config/owlry/style.css` | CSS overrides |
| `~/.local/share/owlry/frecency.json` | Usage history |
| `$XDG_RUNTIME_DIR/owlry/owlry.sock` | IPC socket (overridable via `$OWLRY_SOCKET`) |
| `/usr/share/doc/owlry/owlry.example.lua` | Example Lua configuration |
| `/usr/share/doc/owlry/config.example.toml` | Example TOML configuration |
| `/usr/share/owlry/themes/` | Bundled themes |

Config resolution order (per [`docs/lua-api.md`](docs/lua-api.md) §2): `owlry.lua` → `config.toml` → built-in defaults. When `owlry.lua` is present `config.toml` is ignored entirely (an info log notes this at daemon startup). The daemon watches `owlry.lua` and hot-reloads on save; broken edits surface as desktop notifications and keep the previous state alive.

### Quick Start (Lua config — preferred from 2.1+)

```bash
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/owlry.example.lua ~/.config/owlry/owlry.lua
$EDITOR ~/.config/owlry/owlry.lua
owlry config validate
```

Already have a `config.toml`? Migrate it in one shot:

```bash
owlry migrate-config              # writes owlry.lua; refuses to overwrite
owlry migrate-config --force      # overwrite an existing owlry.lua
```

The migrator is deterministic, emits only values that differ from defaults, and normalises pre-v2 aliases (`system` → `power`, `badge_sys` → `badge_power`).

### Quick Start (TOML config — still supported in 2.x)

```bash
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/config.example.toml ~/.config/owlry/config.toml
$EDITOR ~/.config/owlry/config.toml
owlry config validate
```

### Example Configuration

```toml
[general]
show_icons = true
max_results = 100
tabs = ["app", "cmd", "uuctl"]   # tabs shown in the header bar
# terminal_command = "kitty"      # auto-detected; overrides $TERMINAL and xdg-terminal-exec
# use_uwsm = false                # enable for systemd session integration (uwsm app --)

[appearance]
width = 850
height = 650
font_size = 14
border_radius = 12
# theme = "owl"                   # or: catppuccin-mocha, nord, dracula, ... (see Theming)

# Optional per-element color overrides. All fields are optional; unset inherits from the theme.
# [appearance.colors]
# background = "#1e1e2e"
# accent = "#cba6f7"
# badge_app = "#a6e3a1"            # badge_* keys: app, cmd, clip, ssh, emoji, file,
# badge_web = "#89dceb"            #   power (alias: badge_sys), uuctl, web, calc, bm, dmenu

[providers]
applications = true               # .desktop files
commands = true                   # PATH executables
calculator = true                 # `=` or :calc
converter = true                  # `>` or :conv
power = true                      # `:power` shutdown/reboot/lock (alias: system)
systemd = true                    # `:uuctl` user units (alias: uuctl)
clipboard = true                  # via cliphist
emoji = true                      # picker via wl-clipboard
ssh = true                        # ~/.ssh/config hosts
websearch = true                  # `?` or :web
filesearch = true                 # `/` or :file
frecency = true                   # boost frequently used items
frecency_weight = 0.3             # 0.0 disabled .. 1.0 strong

# Web search engine: google, duckduckgo, bing, startpage, searxng, brave, ecosia
# Or a custom URL with a {query} placeholder: "https://example.com/search?q={query}"
search_engine = "duckduckgo"

# Profiles — named mode sets
[profiles.dev]
modes = ["app", "cmd", "ssh"]
[profiles.minimal]
modes = ["app"]
```

See `/usr/share/doc/owlry/config.example.toml` for every option with documentation.

`owlry config show` prints the resolved effective config (defaults merged with your file). `owlry config validate` parses it and reports errors.

## Theming

### Built-in Themes

| Theme | Description |
|-------|-------------|
| `owl` | Dark theme with amber accents |
| `catppuccin-mocha` | Soothing pastel |
| `nord` | Arctic blue palette |
| `rose-pine` | Natural pine vibes |
| `dracula` | Dark vampire theme |
| `gruvbox-dark` | Retro groove |
| `tokyo-night` | Tokyo city lights |
| `solarized-dark` | Precision colors |
| `one-dark` | Atom's One Dark |
| `apex-neon` | Neon cyberpunk |

```toml
[appearance]
theme = "catppuccin-mocha"
```

### Custom Theme

Create `~/.config/owlry/themes/mytheme.css`:

```css
:root {
    --owlry-bg: #1e1e2e;
    --owlry-bg-secondary: #313244;
    --owlry-border: #45475a;
    --owlry-text: #cdd6f4;
    --owlry-text-secondary: #a6adc8;
    --owlry-accent: #f38ba8;
    --owlry-accent-bright: #f5c2e7;
}
```

### CSS Variables

| Variable | Description |
|----------|-------------|
| `--owlry-bg` | Main background |
| `--owlry-bg-secondary` | Secondary surfaces |
| `--owlry-border` | Border color |
| `--owlry-text` | Primary text |
| `--owlry-text-secondary` | Muted text |
| `--owlry-accent` | Accent color |
| `--owlry-accent-bright` | Bright accent |
| `--owlry-shadow` | Window shadow (default: none) |

## Architecture

```
owlry  (single binary)
├── default invocation         GTK4 UI client (connects to daemon over socket)
├── owlry -d / owlry daemon    IPC daemon (loads providers, listens on the socket)
├── owlry dmenu                stdin → selection (no daemon)
└── owlry doctor / providers / config   diagnostics & config tools

Daemon:
├── Built-in providers     applications, commands, power, calculator, converter
├── Optional providers     bookmarks, clipboard, emoji, ssh, systemd, websearch, filesearch
│                          (compiled in per cargo feature)
├── Frecency tracking      auto-saved every 5 min; flushed on SIGTERM/SIGINT
└── IPC server             $XDG_RUNTIME_DIR/owlry/owlry.sock (newline-delimited JSON)
```

The daemon keeps providers and items warm in memory; the UI launches instantly because there's no work to do at startup. The UI client is a thin GTK4 layer that streams queries and renders results.

Set `OWLRY_SOCKET=/path/to/sock` to override the socket location — useful for running a development daemon alongside a production one.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for feature ideas and [docs/RESTRUCTURE-V2.md](docs/RESTRUCTURE-V2.md) for the v2 rewrite story.

Headline upcoming work:
- **Lua-driven configuration shipped in 2.1** — `~/.config/owlry/owlry.lua` is the canonical config from 2.1, with TOML kept as a back-compat fallback until 3.0. User-defined providers via `owlry.provider {}`, host helpers under `owlry.util.*`, hot reload, named profiles, theme overrides, and `owlry migrate-config` are all live. See [`docs/lua-api.md`](docs/lua-api.md) for the full surface.
- **Widget providers return** — weather, MPRIS media controls, pomodoro timer. Deferred from 2.0 while the UI positioning is reworked.
- **Bookmarks return** — Firefox + Chromium. Deferred from 2.0 to avoid a hard rusqlite/`libsqlite3-sys` dep in the chroot build path; returns with a pure-Rust reader (likely via Firefox's JSON backup files).
- **Dynamic Lua providers** (2.2) — `owlry.provider { dynamic = true }` for per-keystroke items. 2.1 ships only static (`dynamic = false`) providers.

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE).

## Acknowledgments

- [GTK4](https://gtk.org/) — UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) — Wayland Layer Shell
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) — Fuzzy search
- [expr-solver-lib](https://crates.io/crates/expr-solver-lib) — Calculator backend
