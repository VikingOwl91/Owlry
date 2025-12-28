# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A lightweight, owl-themed application launcher for Wayland, built with GTK4 and Layer Shell.

## Features

- **Provider-based architecture** - Search applications, PATH commands, and systemd user services
- **Fuzzy search** - Fast, typo-tolerant matching across all providers
- **Filter tabs & prefixes** - Scope searches with UI tabs or `:app`, `:cmd`, `:uuctl` prefixes
- **Systemd integration** - Manage user services with submenu actions (start/stop/restart/status/journal)
- **GTK4 theming** - Respects system theme by default, with optional custom themes
- **CSS variables** - Full customization via config or custom stylesheets
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
| `↑` / `↓` | Navigate results |
| `Tab` | Cycle filter modes |
| `Shift+Tab` | Cycle filter modes (reverse) |
| `Ctrl+1` | Toggle Applications filter |
| `Ctrl+2` | Toggle Commands filter |
| `Ctrl+3` | Toggle systemd filter |

### Search Prefixes

| Prefix | Scope |
|--------|-------|
| `:app ` | Applications only |
| `:cmd ` | PATH commands only |
| `:uuctl ` | systemd user services only |

Example: `:cmd git` searches only PATH commands for "git"

## Configuration

Configuration file: `~/.config/owlry/config.toml`

An example config is installed at `/usr/share/doc/owlry/config.example.toml`:

```bash
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/config.example.toml ~/.config/owlry/config.toml
```

```toml
[general]
show_icons = true
max_results = 10
# terminal_command = "kitty"  # Auto-detected if not set
# launch_wrapper = "uwsm app --"  # Auto-detected for uwsm/hyprland sessions

[appearance]
width = 600
height = 400
font_size = 14
border_radius = 12
# theme = "owl"  # Optional: "owl" or custom theme name

# [appearance.colors]
# Override individual colors (optional)
# accent = "#e0af68"
# background = "#1a1b26"

[providers]
applications = true
commands = true
uuctl = true
```

### Default Values

| Setting | Default |
|---------|---------|
| `show_icons` | `true` |
| `max_results` | `10` |
| `terminal_command` | Auto-detected ($TERMINAL → xdg-terminal-exec → kitty/alacritty/etc) |
| `launch_wrapper` | Auto-detected (uwsm → hyprctl → none) |
| `width` | `600` |
| `height` | `400` |
| `font_size` | `14` |
| `border_radius` | `12` |
| `theme` | None (GTK default) |

### Launch Wrapper

When running in uwsm-managed or Hyprland sessions, owlry auto-detects and uses the appropriate launch wrapper for proper session integration:

| Session | Wrapper | Purpose |
|---------|---------|---------|
| uwsm | `uwsm app --` | Proper systemd scope and session management |
| Hyprland | `hyprctl dispatch exec --` | Native Hyprland window management |
| Other | None (direct `sh -c`) | Standard shell execution |

You can override this with `launch_wrapper` in config, or set to empty string `""` to disable.

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

- `owl.css` - Owl-inspired dark theme with amber accents
- `catppuccin-mocha.css` - Soothing pastel theme
- `nord.css` - Arctic, north-bluish palette

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

Then reference it in config:

```toml
[appearance]
theme = "mytheme"
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
| `--owlry-badge-app` | Application badge color |
| `--owlry-badge-cmd` | Command badge color |
| `--owlry-badge-dmenu` | Dmenu badge color |
| `--owlry-badge-uuctl` | systemd badge color |

### Custom Stylesheet

For full control, create `~/.config/owlry/style.css` with any GTK4 CSS.

## Providers

### Applications

Searches `.desktop` files from standard XDG directories.

### Commands

Searches executable files in `$PATH`.

### systemd User Services

Lists and controls user-level systemd services. Select a service to access actions:

- Start / Stop / Restart / Reload
- Kill (force stop)
- Status (opens in terminal)
- Journal (live logs in terminal)
- Enable / Disable (autostart)

## License

This project is licensed under the GNU General Public License v3.0 or later - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [GTK4](https://gtk.org/) - UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) - Wayland Layer Shell bindings
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) - Fuzzy search algorithm
