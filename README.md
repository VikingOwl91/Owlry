# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A lightweight, owl-themed application launcher for Wayland, built with GTK4 and Layer Shell.

## Features

- **Provider-based architecture** - Search applications, commands, system actions, SSH hosts, clipboard history, bookmarks, emoji, and more
- **Fuzzy search** - Fast, typo-tolerant matching across all providers
- **Filter tabs & prefixes** - Scope searches with UI tabs or `:app`, `:cmd`, `:sys` prefixes
- **Calculator** - Quick math with `= 5+3` or `calc sin(pi/2)`
- **Web search** - Search the web with `? query` or `web query`
- **File search** - Find files with `/ filename` or `find config` (requires `fd` or `locate`)
- **Frecency ranking** - Frequently/recently used items rank higher
- **GTK4 theming** - Respects system theme by default, with optional custom themes
- **Wayland native** - Uses Layer Shell for proper overlay behavior

## Installation

### Arch Linux (AUR)

```bash
# Using yay
yay -S owlry

# Using paru
paru -S owlry
```

### Build from source

#### Dependencies

```bash
# Arch Linux
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu/Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

#### Optional dependencies

```bash
# For clipboard history
sudo pacman -S cliphist wl-clipboard

# For file search
sudo pacman -S fd  # or: mlocate
```

#### Build

Requires Rust 1.90 or later.

```bash
git clone https://somegit.dev/Owlibou/owlry.git
cd owlry
cargo build --release
```

The binary will be at `target/release/owlry`.

## Usage

```bash
# Launch with default settings (GTK theme, all providers)
owlry

# Launch with only applications
owlry --mode app

# Launch with specific providers
owlry --providers app,cmd

# Show help
owlry --help
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Launch selected item |
| `Escape` | Close launcher / exit submenu |
| `Up` / `Down` | Navigate results |
| `Tab` | Cycle filter modes |
| `Shift+Tab` | Cycle filter modes (reverse) |
| `Ctrl+1` | Toggle Applications filter |
| `Ctrl+2` | Toggle Commands filter |
| `Ctrl+3` | Toggle systemd filter |

### Search Prefixes

Filter results by provider using prefixes:

| Prefix | Provider | Example |
|--------|----------|---------|
| `:app` | Applications | `:app firefox` |
| `:cmd` | PATH commands | `:cmd git` |
| `:sys` | System actions | `:sys shutdown` |
| `:ssh` | SSH hosts | `:ssh server` |
| `:clip` | Clipboard history | `:clip password` |
| `:bm` | Browser bookmarks | `:bm github` |
| `:emoji` | Emoji picker | `:emoji heart` |
| `:script` | Custom scripts | `:script backup` |
| `:file` | File search | `:file config.toml` |
| `:calc` | Calculator | `:calc 5+3` |
| `:web` | Web search | `:web rust docs` |
| `:uuctl` | systemd services | `:uuctl docker` |

### Trigger Prefixes

Some providers can be triggered directly without filter mode:

| Trigger | Provider | Example |
|---------|----------|---------|
| `=` | Calculator | `= 5+3` or `=5*2` |
| `calc ` | Calculator | `calc sqrt(16)` |
| `?` | Web search | `? rust programming` |
| `web ` | Web search | `web linux tips` |
| `search ` | Web search | `search owlry` |
| `/` | File search | `/ .bashrc` |
| `find ` | File search | `find config` |

## Providers

### Applications
Searches `.desktop` files from standard XDG directories.

### Commands
Searches executable files in `$PATH`.

### System
Quick access to system actions:
- Shutdown, Reboot, Suspend, Hibernate
- Lock Screen, Log Out
- **Reboot into BIOS** - Restart directly into UEFI/BIOS setup

### SSH
Parses `~/.ssh/config` and offers quick connections to configured hosts. Opens in your configured terminal.

### Clipboard (requires cliphist)
Search and paste from clipboard history. Requires `cliphist` and `wl-clipboard`:
```bash
sudo pacman -S cliphist wl-clipboard
```

### Bookmarks
Reads bookmarks from Chromium-based browsers:
- Chrome, Chromium, Brave, Edge, Vivaldi

### Emoji
Search 300+ emojis by name or keywords. Selected emoji is copied to clipboard via `wl-copy`.

### Scripts
Runs executable scripts from `~/.config/owlry/scripts/`. Create the directory and add your scripts:
```bash
mkdir -p ~/.config/owlry/scripts
echo '#!/bin/bash
# My backup script
rsync -av ~/Documents /backup/' > ~/.config/owlry/scripts/backup
chmod +x ~/.config/owlry/scripts/backup
```

### Calculator
Evaluate math expressions with `= expr` or `calc expr`:
- Basic: `= 5+3`, `= 10/3`
- Functions: `= sqrt(16)`, `= sin(pi/2)`
- Constants: `= pi`, `= e`

### Web Search
Search the web with `? query` or `web query`. Configurable search engine:
- Google, DuckDuckGo, Bing, Brave, Ecosia, Startpage, SearXNG
- Or custom URL with `{query}` placeholder

### File Search (requires fd or locate)
Search files with `/ pattern` or `find pattern`:
```bash
sudo pacman -S fd  # recommended, faster
# or
sudo pacman -S mlocate && sudo updatedb
```

### systemd User Services
Lists and controls user-level systemd services. Select a service to access actions:
- Start / Stop / Restart / Reload
- Kill (force stop)
- Status (opens in terminal)
- Journal (live logs in terminal)
- Enable / Disable (autostart)

## Configuration

Configuration file: `~/.config/owlry/config.toml`

```bash
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/config.example.toml ~/.config/owlry/config.toml
```

```toml
[general]
show_icons = true
max_results = 10
# terminal_command = "kitty"  # Auto-detected if not set
# launch_wrapper = "uwsm app --"  # Auto-detected for uwsm/hyprland

[appearance]
width = 600
height = 400
font_size = 14
border_radius = 12
# theme = "owl"  # Optional: "owl" or custom theme name

[providers]
applications = true
commands = true
uuctl = true
calculator = true
websearch = true
search_engine = "duckduckgo"  # google, bing, brave, ecosia, startpage, searxng
system = true
ssh = true
clipboard = true
bookmarks = true
emoji = true
scripts = true
files = true
frecency = true
frecency_weight = 0.3  # 0.0 = disabled, 1.0 = strong boost
```

### Default Values

| Setting | Default |
|---------|---------|
| `show_icons` | `true` |
| `max_results` | `10` |
| `terminal_command` | Auto-detected ($TERMINAL -> xdg-terminal-exec -> kitty/alacritty/etc) |
| `launch_wrapper` | Auto-detected (uwsm -> hyprctl -> none) |
| `width` | `600` |
| `height` | `400` |
| `font_size` | `14` |
| `border_radius` | `12` |
| `theme` | None (GTK default) |

### Launch Wrapper

When running in uwsm-managed or Hyprland sessions, owlry auto-detects and uses the appropriate launch wrapper:

| Session | Wrapper | Purpose |
|---------|---------|---------|
| uwsm | `uwsm app --` | Proper systemd scope and session management |
| Hyprland | `hyprctl dispatch exec --` | Native Hyprland window management |
| Other | None (direct `sh -c`) | Standard shell execution |

## Theming

### GTK Theme (Default)

By default, Owlry inherits colors from your system GTK4 theme (Adwaita, Breeze, etc.).

### Built-in Themes

Owlry includes an owl-inspired dark theme:

```toml
[appearance]
theme = "owl"
```

### Included Example Themes

Example themes are installed to `/usr/share/owlry/themes/`:

| Theme | Description |
|-------|-------------|
| `owl` | Owl-inspired dark theme with amber accents |
| `catppuccin-mocha` | Soothing pastel theme |
| `nord` | Arctic, north-bluish palette |
| `rose-pine` | All natural pine, faux fur and soho vibes |
| `dracula` | Dark theme for vampires |
| `gruvbox-dark` | Retro groove color scheme |
| `tokyo-night` | Lights of Tokyo at night |
| `solarized-dark` | Precision colors for machines and people |
| `one-dark` | Atom's iconic One Dark theme |

To use an example theme:

```bash
mkdir -p ~/.config/owlry/themes
cp /usr/share/owlry/themes/catppuccin-mocha.css ~/.config/owlry/themes/
```

Then set in config:

```toml
[appearance]
theme = "catppuccin-mocha"
```

### Custom Theme

Create a custom theme file at `~/.config/owlry/themes/mytheme.css`:

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

### CSS Variables Reference

| Variable | Description |
|----------|-------------|
| `--owlry-bg` | Main background |
| `--owlry-bg-secondary` | Secondary surfaces |
| `--owlry-border` | Border color |
| `--owlry-text` | Primary text |
| `--owlry-text-secondary` | Muted text |
| `--owlry-accent` | Accent/highlight color |
| `--owlry-accent-bright` | Bright accent |
| `--owlry-font-size` | Base font size |
| `--owlry-border-radius` | Border radius |
| `--owlry-badge-*` | Provider badge colors (app, cmd, sys, ssh, clip, emoji, etc.) |

### Custom Stylesheet

For full control, create `~/.config/owlry/style.css` with any GTK4 CSS.

## License

This project is licensed under the GNU General Public License v3.0 or later - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [GTK4](https://gtk.org/) - UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) - Wayland Layer Shell bindings
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) - Fuzzy search algorithm
