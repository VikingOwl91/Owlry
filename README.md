# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A lightweight, owl-themed application launcher for Wayland, built with GTK4 and Layer Shell.

## Features

- **Modular plugin architecture** — Install only what you need
- **Fuzzy search with tags** — Fast matching across names, descriptions, and category tags
- **13 native plugins** — Calculator, clipboard, emoji, weather, media, and more
- **Widget providers** — Weather, media controls, and pomodoro timer at the top of results
- **Filter prefixes** — Scope searches with `:app`, `:cmd`, `:tag:development`, etc.
- **Frecency ranking** — Frequently/recently used items rank higher
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
yay -S owlry-meta-essentials  # calculator, system, ssh, scripts, bookmarks
yay -S owlry-meta-widgets     # weather, media, pomodoro
yay -S owlry-meta-tools       # clipboard, emoji, websearch, filesearch, systemd
yay -S owlry-meta-full        # everything

# For custom Lua/Rune plugins
yay -S owlry-lua   # Lua 5.4 runtime
yay -S owlry-rune  # Rune runtime
```

### Available Packages

| Package | Description |
|---------|-------------|
| `owlry` | Core binary with applications and commands |
| `owlry-plugin-calculator` | Math expressions (`= 5+3`) |
| `owlry-plugin-system` | Shutdown, reboot, suspend, lock |
| `owlry-plugin-ssh` | SSH hosts from `~/.ssh/config` |
| `owlry-plugin-clipboard` | History via cliphist |
| `owlry-plugin-emoji` | 400+ searchable emoji |
| `owlry-plugin-scripts` | User scripts |
| `owlry-plugin-bookmarks` | Firefox, Chrome, Brave, Edge bookmarks |
| `owlry-plugin-websearch` | Web search (`? query`) |
| `owlry-plugin-filesearch` | File search (`/ filename`) |
| `owlry-plugin-systemd` | User services with actions |
| `owlry-plugin-weather` | Weather widget |
| `owlry-plugin-media` | MPRIS media controls |
| `owlry-plugin-pomodoro` | Pomodoro timer widget |

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

# Build core only
cargo build --release -p owlry

# Build specific plugin
cargo build --release -p owlry-plugin-calculator

# Build everything
cargo build --release --workspace
```

**Install plugins manually:**
```bash
sudo mkdir -p /usr/lib/owlry/plugins
sudo cp target/release/libowlry_plugin_*.so /usr/lib/owlry/plugins/
```

## Usage

```bash
owlry                      # Launch with all providers
owlry -m app               # Applications only
owlry -m cmd               # PATH commands only
owlry -p app,cmd           # Multiple specific providers
owlry -m calc              # Calculator plugin only (if installed)
owlry --help               # Show all options with examples
```

### dmenu Mode

Owlry is dmenu-compatible. Pipe input for interactive selection:

```bash
# Basic selection
echo -e "Option A\nOption B\nOption C" | owlry -m dmenu

# Select from files
ls ~/Documents | owlry -m dmenu

# Git branch checkout
git branch | owlry -m dmenu --prompt "checkout:" | xargs git checkout

# Kill a process
ps -eo comm | sort -u | owlry -m dmenu --prompt "kill:" | xargs pkill

# Select and open a project
find ~/projects -maxdepth 1 -type d | owlry -m dmenu | xargs code

# Package manager search
pacman -Ssq | owlry -m dmenu --prompt "install:" | xargs sudo pacman -S
```

The `--prompt` flag sets a custom label for the search input.

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
# terminal_command = "kitty"    # Auto-detected
# launch_wrapper = "uwsm app --" # Auto-detected

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
```

See `/usr/share/doc/owlry/config.example.toml` for all options with documentation.

## Plugin System

Owlry uses a modular plugin architecture. Plugins are loaded from:

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

```
owlry (core)
├── Applications provider (XDG .desktop files)
├── Commands provider (PATH executables)
├── Dmenu provider (pipe compatibility)
└── Plugin loader
    ├── /usr/lib/owlry/plugins/*.so  (native plugins)
    ├── /usr/lib/owlry/runtimes/     (Lua/Rune runtimes)
    └── ~/.config/owlry/plugins/     (user plugins)
```

For detailed architecture information, see [CLAUDE.md](CLAUDE.md).

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE).

## Acknowledgments

- [GTK4](https://gtk.org/) — UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) — Wayland Layer Shell
- [abi_stable](https://crates.io/crates/abi_stable) — ABI-stable Rust plugins
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) — Fuzzy search
