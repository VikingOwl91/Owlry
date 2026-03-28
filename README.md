# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A lightweight, owl-themed application launcher for Wayland, built with GTK4 and Layer Shell.

## Features

- **Client/daemon architecture** — Instant window appearance, providers stay loaded in memory
- **Modular plugin architecture** — Install only what you need
- **Fuzzy search with tags** — Fast matching across names, descriptions, and category tags
- **14 native plugins** — Calculator, clipboard, emoji, weather, media, and more
- **Widget providers** — Weather, media controls, and pomodoro timer at the top of results
- **Config profiles** — Named mode presets for different workflows
- **Filter prefixes** — Scope searches with `:app`, `:cmd`, `:tag:development`, etc.
- **Frecency ranking** — Frequently/recently used items rank higher
- **Toggle behavior** — Bind one key to open/close the launcher
- **GTK4 theming** — System theme by default, with 9 built-in themes
- **Wayland native** — Uses Layer Shell for proper overlay behavior
- **Extensible** — Create custom plugins in Lua or Rune

## Installation

### Arch Linux (AUR)

```bash
# Minimal core (applications + commands only)
yay -S owlry

# Add individual plugins
yay -S owlry-plugin-calculator owlry-plugin-weather

# Or install bundles:
yay -S owlry-meta-essentials  # calculator, converter, system, ssh, scripts, bookmarks
yay -S owlry-meta-widgets     # weather, media, pomodoro
yay -S owlry-meta-tools       # clipboard, emoji, websearch, filesearch, systemd
yay -S owlry-meta-full        # everything

# For custom Lua/Rune plugins
yay -S owlry-lua   # Lua 5.4 runtime
yay -S owlry-rune  # Rune runtime
```

### Available Packages

**Core packages** (this repo):

| Package | Description |
|---------|-------------|
| `owlry` | GTK4 UI client |
| `owlry-core` | Headless daemon (plugin host, IPC server) |
| `owlry-lua` | Lua 5.4 script runtime for user plugins |
| `owlry-rune` | Rune script runtime for user plugins |

**Plugin packages** ([owlry-plugins](https://somegit.dev/Owlibou/owlry-plugins) repo):

| Package | Description |
|---------|-------------|
| `owlry-plugin-bookmarks` | Firefox, Chrome, Brave, Edge bookmarks |
| `owlry-plugin-calculator` | Math expressions (`= 5+3`) |
| `owlry-plugin-clipboard` | History via cliphist |
| `owlry-plugin-converter` | Unit and currency conversion |
| `owlry-plugin-emoji` | 400+ searchable emoji |
| `owlry-plugin-filesearch` | File search (`/ filename`) |
| `owlry-plugin-media` | MPRIS media controls |
| `owlry-plugin-pomodoro` | Pomodoro timer widget |
| `owlry-plugin-scripts` | User scripts |
| `owlry-plugin-ssh` | SSH hosts from `~/.ssh/config` |
| `owlry-plugin-system` | Shutdown, reboot, suspend, lock |
| `owlry-plugin-systemd` | User services with actions |
| `owlry-plugin-weather` | Weather widget |
| `owlry-plugin-websearch` | Web search (`? query`) |

**Meta bundles:**

| Package | Includes |
|---------|----------|
| `owlry-meta-essentials` | bookmarks, calculator, converter, scripts, ssh, system |
| `owlry-meta-tools` | clipboard, emoji, filesearch, systemd, websearch |
| `owlry-meta-widgets` | media, pomodoro, weather |
| `owlry-meta-full` | All plugins + runtimes |

### Build from Source

**Dependencies:**
```bash
# Arch Linux
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu/Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

**Build (requires Rust 1.90+):**
```bash
git clone https://somegit.dev/Owlibou/owlry.git
cd owlry

# Build daemon + UI
cargo build --release -p owlry -p owlry-core

# Build runtimes (for user plugins)
cargo build --release -p owlry-lua -p owlry-rune

# Build everything in this workspace
cargo build --release --workspace
```

**Plugins** are in a [separate repo](https://somegit.dev/Owlibou/owlry-plugins):
```bash
git clone https://somegit.dev/Owlibou/owlry-plugins.git
cd owlry-plugins
cargo build --release -p owlry-plugin-calculator  # or any plugin
```

**Install locally:**
```bash
just install-local
```

This installs the UI, daemon, runtimes, and systemd service files.

## Getting Started

Owlry uses a client/daemon architecture. The daemon (`owlry-core`) loads providers and plugins into memory. The UI client (`owlry`) connects to the daemon over a Unix socket for instant results.

### Starting the Daemon

Choose one of three methods:

**1. Compositor autostart (recommended for most users)**

Add to your compositor config:

```bash
# Hyprland (~/.config/hypr/hyprland.conf)
exec-once = owlry-core

# Sway (~/.config/sway/config)
exec owlry-core
```

**2. Systemd user service**

```bash
systemctl --user enable --now owlry-core.service
```

**3. Socket activation (auto-start on first use)**

```bash
systemctl --user enable owlry-core.socket
```

The daemon starts automatically when the UI client first connects. No manual startup needed.

### Launching the UI

Bind `owlry` to a key in your compositor:

```bash
# Hyprland
bind = SUPER, Space, exec, owlry

# Sway
bindsym $mod+space exec owlry
```

Running `owlry` a second time while it is already open sends a toggle command — the window closes. This means a single keybind acts as open/close.

If the daemon is not running when the UI launches, it will attempt to start it via systemd automatically.

## Usage

```bash
owlry                      # Launch with all providers
owlry -m app               # Applications only
owlry -m cmd               # PATH commands only
owlry -m calc              # Calculator plugin only (if installed)
owlry --profile dev        # Use a named profile from config
owlry --help               # Show all options with examples
```

### Profiles

Profiles are named sets of modes defined in your config:

```toml
[profiles.dev]
modes = ["app", "cmd", "ssh"]

[profiles.media]
modes = ["media", "emoji"]

[profiles.minimal]
modes = ["app"]
```

Launch with a profile:

```bash
owlry --profile dev
```

You can bind different profiles to different keys:

```bash
# Hyprland
bind = SUPER, Space, exec, owlry
bind = SUPER, D, exec, owlry --profile dev
bind = SUPER, M, exec, owlry --profile media
```

### dmenu Mode

Owlry is dmenu-compatible. Pipe input for interactive selection — the selected item is printed to stdout (not executed), so you pipe the output to execute it.

dmenu mode is self-contained: it does not use the daemon and works without `owlry-core` running.

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

# Package manager search
pacman -Ssq | owlry -m dmenu -p "install" | xargs sudo pacman -S

# Open selected file
ls ~/Documents | owlry -m dmenu | xargs xdg-open
```

The `-p` / `--prompt` flag sets a custom label for the search input.

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
| `:sys` | System actions | `:sys shutdown` |
| `:ssh` | SSH hosts | `:ssh server` |
| `:clip` | Clipboard | `:clip password` |
| `:bm` | Bookmarks | `:bm github` |
| `:emoji` | Emoji | `:emoji heart` |
| `:script` | Scripts | `:script backup` |
| `:file` | Files | `:file config` |
| `:calc` | Calculator | `:calc sqrt(16)` |
| `:web` | Web search | `:web rust docs` |
| `:uuctl` | systemd | `:uuctl docker` |
| `:tag:X` | Filter by tag | `:tag:development` |

### Trigger Prefixes

| Trigger | Provider | Example |
|---------|----------|---------|
| `=` | Calculator | `= 5+3` |
| `calc ` | Calculator | `calc sqrt(16)` |
| `?` | Web search | `? rust programming` |
| `web ` | Web search | `web linux tips` |
| `/` | File search | `/ .bashrc` |
| `find ` | File search | `find config` |

## Configuration

Owlry follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/):

| Path | Purpose |
|------|---------|
| `~/.config/owlry/config.toml` | Main configuration |
| `~/.config/owlry/themes/*.css` | Custom themes |
| `~/.config/owlry/style.css` | CSS overrides |
| `~/.config/owlry/plugins/` | User plugins (Lua/Rune) |
| `~/.local/share/owlry/scripts/` | User scripts |
| `~/.local/share/owlry/frecency.json` | Usage history |

System locations:
| Path | Purpose |
|------|---------|
| `/usr/lib/owlry/plugins/*.so` | Installed native plugins |
| `/usr/lib/owlry/runtimes/*.so` | Lua/Rune script runtimes |
| `/usr/share/doc/owlry/config.example.toml` | Example configuration |

### Quick Start

```bash
# Copy example config
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/config.example.toml ~/.config/owlry/config.toml
```

### Example Configuration

```toml
[general]
show_icons = true
max_results = 10
tabs = ["app", "cmd", "uuctl"]
# terminal_command = "kitty"  # Auto-detected
# use_uwsm = false            # Enable for systemd session integration

[appearance]
width = 850
height = 650
font_size = 14
border_radius = 12
# theme = "owl"  # Or: catppuccin-mocha, nord, dracula, etc.

[plugins]
disabled = []  # Plugin IDs to disable, e.g., ["emoji", "pomodoro"]

[providers]
applications = true    # .desktop files
commands = true        # PATH executables
frecency = true        # Boost frequently used items
frecency_weight = 0.3  # 0.0-1.0

# Web search engine: google, duckduckgo, bing, startpage, brave, ecosia
search_engine = "duckduckgo"

# Profiles: named sets of modes
[profiles.dev]
modes = ["app", "cmd", "ssh"]

[profiles.media]
modes = ["media", "emoji"]
```

See `/usr/share/doc/owlry/config.example.toml` for all options with documentation.

## Plugin System

Owlry uses a modular plugin architecture. Plugins are loaded by the daemon (`owlry-core`) from:

- `/usr/lib/owlry/plugins/*.so` — System plugins (AUR packages)
- `~/.config/owlry/plugins/` — User plugins (requires `owlry-lua` or `owlry-rune`)

### Disabling Plugins

Add plugin IDs to the disabled list in your config:

```toml
[plugins]
disabled = ["emoji", "pomodoro"]
```

### Plugin Management CLI

```bash
# List installed plugins
owlry plugin list
owlry plugin list --enabled        # Only enabled
owlry plugin list --available      # Show registry plugins

# Search registry
owlry plugin search "weather"

# Install/remove
owlry plugin install <name>        # From registry
owlry plugin install ./my-plugin   # From local path
owlry plugin remove <name>

# Enable/disable
owlry plugin enable <name>
owlry plugin disable <name>

# Plugin info
owlry plugin info <name>
owlry plugin commands <name>       # List plugin CLI commands

# Create new plugin
owlry plugin create my-plugin              # Lua (default)
owlry plugin create my-plugin -r rune      # Rune

# Run plugin command
owlry plugin run <plugin-id> <command> [args...]
```

### Creating Custom Plugins

See [docs/PLUGIN_DEVELOPMENT.md](docs/PLUGIN_DEVELOPMENT.md) for:
- Native plugin development (Rust)
- Lua plugin development
- Rune plugin development
- Available APIs

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

## Architecture

Owlry uses a client/daemon split:

```
owlry-core (daemon)                    owlry (GTK4 UI client)
├── Loads config + plugins             ├── Connects to daemon via Unix socket
├── Applications provider              ├── Renders results in GTK4 window
├── Commands provider                  ├── Handles keyboard input
├── Plugin loader                      ├── Toggle: second launch closes window
│   ├── /usr/lib/owlry/plugins/*.so    └── dmenu mode (self-contained, no daemon)
│   ├── /usr/lib/owlry/runtimes/
│   └── ~/.config/owlry/plugins/
├── Frecency tracking
└── IPC server (Unix socket)
         │
         └── $XDG_RUNTIME_DIR/owlry/owlry.sock
```

The daemon keeps providers and plugins loaded in memory, so the UI appears instantly when launched. The UI client is a thin GTK4 layer that sends queries and receives results over the socket.

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE).

## Acknowledgments

- [GTK4](https://gtk.org/) — UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) — Wayland Layer Shell
- [abi_stable](https://crates.io/crates/abi_stable) — ABI-stable Rust plugins
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) — Fuzzy search
