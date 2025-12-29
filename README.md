# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A lightweight, owl-themed application launcher for Wayland, built with GTK4 and Layer Shell.

## Features

- **Provider-based architecture** — Search applications, commands, system actions, SSH hosts, clipboard history, bookmarks, emoji, and more
- **Fuzzy search with tags** — Fast matching across names, descriptions, and category tags
- **Configurable tabs** — Customize header tabs and keyboard shortcuts
- **Filter prefixes** — Scope searches with `:app`, `:cmd`, `:tag:development`, etc.
- **Calculator** — Quick math with `= 5+3` or `calc sin(pi/2)`
- **Web search** — Search the web with `? query`
- **File search** — Find files with `/ filename` (requires `fd` or `locate`)
- **Frecency ranking** — Frequently/recently used items rank higher
- **GTK4 theming** — System theme by default, with 9 built-in themes
- **Wayland native** — Uses Layer Shell for proper overlay behavior

## Installation

### Arch Linux (AUR)

```bash
yay -S owlry
# or
paru -S owlry
```

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

**Optional dependencies:**
```bash
# Clipboard history
sudo pacman -S cliphist wl-clipboard

# File search (choose one)
sudo pacman -S fd       # recommended
sudo pacman -S mlocate  # alternative
```

**Build (requires Rust 1.90+):**
```bash
git clone https://somegit.dev/Owlibou/owlry.git
cd owlry
cargo build --release
# Binary: target/release/owlry
```

## Usage

```bash
owlry                      # Launch with defaults
owlry --mode app           # Applications only
owlry --providers app,cmd  # Specific providers
owlry --help               # Show all options
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Launch selected item |
| `Escape` | Close launcher / exit submenu |
| `↑` / `↓` | Navigate results |
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

## File Locations

Owlry follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/):

| Path | Purpose |
|------|---------|
| `~/.config/owlry/config.toml` | Main configuration |
| `~/.config/owlry/themes/*.css` | Custom themes |
| `~/.config/owlry/style.css` | CSS overrides |
| `~/.local/share/owlry/scripts/` | User scripts |
| `~/.local/share/owlry/frecency.json` | Usage history |

## Configuration

Copy the example files:
```bash
# Config
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/config.example.toml ~/.config/owlry/config.toml

# Optional: CSS overrides
cp /usr/share/doc/owlry/style.example.css ~/.config/owlry/style.css

# Optional: Example script
mkdir -p ~/.local/share/owlry/scripts
cp /usr/share/doc/owlry/scripts/example.sh ~/.local/share/owlry/scripts/
```

### Example Configuration

```toml
[general]
show_icons = true
max_results = 10
tabs = ["app", "cmd", "uuctl"]  # Header tabs (Ctrl+1, Ctrl+2, etc.)
# terminal_command = "kitty"    # Auto-detected
# launch_wrapper = "uwsm app --" # Auto-detected

[appearance]
width = 600
height = 400
font_size = 14
border_radius = 12
# theme = "owl"  # Or: catppuccin-mocha, nord, dracula, etc.

[providers]
applications = true
commands = true
uuctl = true
calculator = true
websearch = true
search_engine = "duckduckgo"
system = true
ssh = true
clipboard = true
bookmarks = true
emoji = true
scripts = true
files = true
frecency = true
frecency_weight = 0.3
```

### Tab Configuration

Customize which providers appear as header tabs:

```toml
[general]
# Available: app, cmd, uuctl, bookmark, calc, clip, dmenu,
#            emoji, file, script, ssh, sys, web
tabs = ["app", "cmd", "ssh", "sys"]
```

Keyboard shortcuts `Ctrl+1` through `Ctrl+9` map to tab positions.

## Providers

| Provider | Description | Trigger |
|----------|-------------|---------|
| **Applications** | `.desktop` files from XDG directories | `:app` |
| **Commands** | Executables in `$PATH` | `:cmd` |
| **System** | Shutdown, reboot, suspend, lock, BIOS | `:sys` |
| **SSH** | Hosts from `~/.ssh/config` | `:ssh` |
| **Clipboard** | History via cliphist | `:clip` |
| **Bookmarks** | Chrome, Brave, Edge, Vivaldi | `:bm` |
| **Emoji** | 300+ searchable emoji | `:emoji` |
| **Scripts** | User scripts | `:script` |
| **Calculator** | Math expressions | `=` or `:calc` |
| **Web Search** | Configurable engine | `?` or `:web` |
| **Files** | fd/locate search | `/` or `:file` |
| **systemd** | User services with actions | `:uuctl` |

### Tags

Items are tagged for better search:
- **Applications**: Categories from `.desktop` files (development, utility, etc.)
- **System**: `power`, `system`
- **SSH**: `ssh`
- **Scripts**: `script`
- **systemd**: `systemd`, `service`

Filter by tag with `:tag:tagname`:
```
:tag:development     # Show development apps
:tag:utility vim     # Search utilities for "vim"
```

### Scripts

Create executable scripts in `~/.local/share/owlry/scripts/`:

```bash
mkdir -p ~/.local/share/owlry/scripts
cat > ~/.local/share/owlry/scripts/backup.sh << 'EOF'
#!/bin/bash
rsync -av ~/Documents /backup/
notify-send "Backup complete"
EOF
chmod +x ~/.local/share/owlry/scripts/backup.sh
```

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

### CSS Overrides

For tweaks without a full theme, create `~/.config/owlry/style.css`:

```css
/* Larger search input */
.owlry-search {
    font-size: 18px;
    padding: 12px 16px;
}

/* Hide tag badges */
.owlry-tag-badge {
    display: none;
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
| `--owlry-font-size` | Base font size |
| `--owlry-border-radius` | Corner radius |

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE).

## Acknowledgments

- [GTK4](https://gtk.org/) — UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) — Wayland Layer Shell
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) — Fuzzy search
