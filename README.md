# Owlry

[![AUR](https://img.shields.io/aur/version/owlry?logo=archlinux&label=AUR)](https://aur.archlinux.org/packages/owlry)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
[![GTK4](https://img.shields.io/badge/GTK-4.12-green.svg)](https://gtk.org/)
[![Wayland](https://img.shields.io/badge/Wayland-native-blueviolet.svg)](https://wayland.freedesktop.org/)

A fast, keyboard-first application launcher for Wayland. Owlry runs a small
GTK4 client over a warm daemon, so the window opens immediately and results
are already ranked. One binary provides the UI, daemon, and every provider.

<p align="center">
  <img src="assets/owlry-demo.gif" alt="Owlry searching applications, calculating, converting units, and showing power actions" width="1000">
</p>

Owlry is built for people who want a launcher that stays out of the way. It
ships as a single binary, keeps providers warm in memory, and does not build
shell command strings from ordinary results. Configuration is Lua-first in the
AUR build, with TOML kept as a fallback through 2.x.

> **Upgrading to 2.8.0?** Restart `owlry.service` after the package upgrade.
> The IPC activation schema changed, and the UI and daemon must run the same
> version. See [Upgrading](docs/upgrading.md) and the
> [2.8.0 release notes](https://somegit.dev/Owlibou/owlry/releases/tag/owlry-v2.8.0).

## Highlights

- **Instant UI.** A warm daemon keeps provider data in memory, so opening the
  launcher is a socket round-trip rather than a cold scan.
- **One binary.** The UI, daemon, providers, and diagnostics all live in
  `/usr/bin/owlry`.
- **Useful defaults.** Applications, PATH commands, calculator, unit and
  currency converter, power actions, and dmenu mode are always built in. The
  default feature set adds systemd user units.
- **Optional providers.** Clipboard history, emoji, rofimoji, SSH hosts, web
  search, and filesystem search are compiled in with cargo features. The AUR
  package enables all of them.
- **Search that stays useful.** Fuzzy matching across names, descriptions, and
  tags, with frecency ranking and `:recent`.
- **Lua configuration.** Profiles, themes, user-defined providers, and hot
  reload. TOML remains supported through 2.x.
- **Wayland native.** GTK4 and Layer Shell, with ten bundled themes.

## Install

### Arch Linux (AUR)

```bash
paru -Syu owlry     # or: yay -Syu owlry
systemctl --user restart owlry.service
```

The restart is required once when upgrading to 2.8.0. The package prints a
reminder after the upgrade, but it does not restart the daemon for you.

Upgrading from 1.x? The package replaces the old `owlry-*` packages
automatically. See [Upgrading](docs/upgrading.md) for the unit rename and
migration notes.

### Build from source

Install GTK4 and gtk4-layer-shell, then build with the feature set you want:

```bash
git clone https://somegit.dev/Owlibou/owlry.git
cd owlry
cargo build --release --features full
just install-local    # development install of the binary and systemd units
```

`--features full` matches the AUR build. Core providers are always compiled;
optional features are `clipboard`, `emoji`, `rofimoji`, `filesearch`, `ssh`,
`systemd`, `websearch`, and `lua`. A default build enables only `systemd`.
See [Contributing](CONTRIBUTING.md) for distro dependencies and the test
matrix.

## Quick start

```bash
# Recommended: run the daemon as a systemd user service
systemctl --user enable --now owlry.service
```

Bind `owlry` to a key:

```text
# Hyprland
bind = SUPER, Space, exec, owlry

# Sway
bindsym $mod+space exec owlry
```

Running `owlry` while a window is open sends a toggle command, so one keybind
opens and closes the launcher.

Try these in the search field:

```text
firefox
= 1920/1080
> 20 km to mi
:power
? wayland launcher
:shell printf '%s\n' foo | wl-copy
```

Typing `program args` launches that argument vector without a shell. Shell
syntax requires the explicit `:shell` prefix. `:cmd` remains a PATH-command
filter and does not enable shell syntax.

## Configuration

Owlry reads `~/.config/owlry/owlry.lua` first, then
`~/.config/owlry/config.toml`, then built-in defaults. The Lua layer is
included in the AUR build and in any build with `--features lua`.

```lua
owlry.set { theme = "owl", width = 900, height = 700 }
owlry.providers { "app", "cmd", "calc", "conv", "power", "systemd" }
owlry.tabs { "app", "cmd", "uuctl" }

owlry.profiles {
  dev = { "app", "cmd", "ssh" },
}
```

```bash
owlry config validate   # parse the active config
owlry config show       # print the resolved effective config
owlry migrate-config    # TOML to owlry.lua
```

See [Configuration](docs/configuration.md) for the full option reference and
[Lua API](docs/lua-api.md) for the config surface, user providers, and host
helpers.

## Theming

Ten themes ship with Owlry: `owl`, `catppuccin-mocha`, `nord`, `rose-pine`,
`dracula`, `gruvbox-dark`, `tokyo-night`, `solarized-dark`, `one-dark`, and
`apex-neon`. Pick one with `owlry.set { theme = "nord" }` or override colors
in `~/.config/owlry/themes/*.css`. See [Theming](docs/theming.md).

## Documentation

| Guide | Covers |
|---|---|
| [Usage](docs/usage.md) | CLI, dmenu mode, keyboard shortcuts, prefixes, `:shell` |
| [Configuration](docs/configuration.md) | Lua and TOML options, profiles, paths, migration |
| [Lua API](docs/lua-api.md) | `owlry.set`, providers, tabs, themes, profiles, host helpers |
| [Theming](docs/theming.md) | Bundled themes, custom themes, CSS variables |
| [File opening](docs/file-opening.md) | Default and terminal handlers, open-with picker |
| [Architecture](docs/architecture.md) | Daemon, IPC, provider lifecycle, performance |
| [Upgrading](docs/upgrading.md) | 1.x package migration, 2.8.0 restart, mixed versions |
| [Provider freshness](docs/provider-freshness.md) | Application and systemd state refresh |
| [Rofimoji](docs/rofimoji.md) | Emoji and kaomoji picker integration |
| [Dependency audit](docs/dependency-audit.md) | RustSec exceptions and removal conditions |
| [Changelog](CHANGELOG.md) | Release history |
| [Roadmap](ROADMAP.md) | Planned work |

## Contributing

Issues and pull requests are welcome on
[Gitea](https://somegit.dev/Owlibou/owlry). Before opening a pull request,
run:

```bash
just verify
```

That covers formatting, `cargo check`, tests on both feature axes, strict
Clippy, and `cargo audit`. The development toolchain is pinned in
`rust-toolchain.toml`; the crate MSRV is Rust 1.90. Commit messages follow the
existing conventional style, and changes should stay focused.

For security issues, contact the maintainer privately rather than opening a
public issue. See [Contributing](CONTRIBUTING.md) for the full workflow.

## License

GNU General Public License v3.0 or later. See [LICENSE](LICENSE).

## Acknowledgments

- [GTK4](https://gtk.org/) for the UI toolkit
- [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell) for Wayland Layer Shell
- [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) for fuzzy matching
- [expr-solver-lib](https://crates.io/crates/expr-solver-lib) for the calculator
