# Configuration

Owlry follows the XDG Base Directory specification. The Lua configuration is
preferred; TOML remains supported through 2.x.

## File locations

| Path | Purpose |
|---|---|
| `~/.config/owlry/owlry.lua` | Lua configuration (preferred) |
| `~/.config/owlry/config.toml` | TOML configuration (legacy; ignored when `owlry.lua` exists) |
| `~/.config/owlry/themes/*.css` | Custom themes |
| `~/.config/owlry/style.css` | CSS overrides |
| `~/.local/share/owlry/frecency.json` | Usage history, mode 0600 |
| `$XDG_RUNTIME_DIR/owlry/owlry.sock` | IPC socket; override with `$OWLRY_SOCKET` |
| `/usr/share/doc/owlry/owlry.example.lua` | Example Lua configuration from the package |
| `/usr/share/doc/owlry/config.example.toml` | Example TOML configuration from the package |
| `/usr/share/owlry/themes/` | Bundled themes |

Resolution order is `owlry.lua`, then `config.toml`, then built-in defaults.
When `owlry.lua` is present, `config.toml` is ignored entirely; the daemon
logs this at startup.

The Lua layer is compiled with the `lua` feature. The AUR package builds with
`--features full`, so Lua is available there. A minimal source build without
`lua` reads TOML only.

## Quick start: Lua

```bash
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/owlry.example.lua ~/.config/owlry/owlry.lua
$EDITOR ~/.config/owlry/owlry.lua
owlry config validate
```

A minimal configuration:

```lua
owlry.set {
  theme = "owl",
  width = 900,
  height = 700,
  max_results = 100,
}

owlry.providers { "app", "cmd", "calc", "conv", "power", "systemd" }
owlry.tabs { "app", "cmd", "uuctl" }

owlry.profiles {
  dev = { "app", "cmd", "ssh" },
}
```

The daemon watches `owlry.lua` and Lua modules below the config directory and
hot-reloads on save. A broken edit produces a notification and keeps the
previous state running.

See [Lua API](lua-api.md) for `owlry.set`, `owlry.providers`, `owlry.tabs`,
`owlry.theme`, `owlry.profiles`, `owlry.provider`, and the `owlry.util.*`
helpers.

## Quick start: TOML

```bash
mkdir -p ~/.config/owlry
cp /usr/share/doc/owlry/config.example.toml ~/.config/owlry/config.toml
$EDITOR ~/.config/owlry/config.toml
owlry config validate
```

## Example TOML

```toml
[general]
show_icons = true
max_results = 100
tabs = ["app", "cmd", "uuctl"]
# terminal_command = "kitty"      # auto-detected; overrides $TERMINAL and xdg-terminal-exec
# use_uwsm = false                # enable for systemd session integration (uwsm app --)

[appearance]
width = 850
height = 650
font_size = 14
border_radius = 12
# theme = "owl"                   # or: catppuccin-mocha, nord, dracula, ...
# style.css in ~/.config/owlry overrides individual values

# Optional per-element color overrides. All fields are optional.
# [appearance.colors]
# background = "#1e1e2e"
# accent = "#cba6f7"
# badge_app = "#a6e3a1"            # badge_* keys: app, cmd, clip, ssh, emoji,
# badge_web = "#89dceb"            #   file, power (alias: badge_sys), uuctl,
#                                  #   web, calc, dmenu

[providers]
applications = true
commands = true
calculator = true
converter = true
power = true
systemd = true
clipboard = true
emoji = true
rofimoji = true
ssh = true
websearch = true
filesearch = true

frecency = true
frecency_weight = 0.3             # 0.0 disabled, 1.0 strong
frecency_max_entries = 5000       # cap retained entries (0 = unbounded)
frecency_max_age_days = 180       # drop older entries (0 = off)
frecency_min_launches = 3         # launches that exempt an old entry from pruning

# google, duckduckgo, bing, startpage, searxng, brave, ecosia, or a custom
# URL with a {query} placeholder.
search_engine = "duckduckgo"

# Root directories for file search (the :file prefix or / alias). "~/" is
# expanded; non-existent entries are dropped. Empty searches $HOME.
# filesearch_roots = ["~/Documents", "~/code"]

[profiles.dev]
modes = ["app", "cmd", "ssh"]

[profiles.minimal]
modes = ["app"]
show_all_tab = false
```

The v1 `[plugins]` table is no longer used. Provider availability is a
compile-time cargo feature; runtime enablement is controlled by `[providers]`.

## Profiles

Profiles select a set of provider modes and are chosen with
`owlry --profile <name>`.

```toml
[profiles.dev]
modes = ["app", "cmd", "ssh"]

[profiles.media]
modes = ["emoji", "clipboard"]
show_all_tab = false
```

`show_all_tab = false` keeps Tab cycling inside the profile's tabs. The
default is `true`.

## Frecency

Frecency ranks frequently and recently launched items. The store is written
to `~/.local/share/owlry/frecency.json` with mode 0600, and the daemon rejects
symlinks, hardlinks, FIFOs, and files owned by another user.

| Option | Default | Meaning |
|---|---|---|
| `frecency` | `true` | Enable frecency ranking |
| `frecency_weight` | `0.3` | Blend weight, 0.0 to 1.0 |
| `frecency_max_entries` | `5000` | Retained entries, 0 for unbounded |
| `frecency_max_age_days` | `180` | Age limit, 0 to disable |
| `frecency_min_launches` | `3` | Launches that exempt an old entry from pruning |

## Migration from TOML

```bash
owlry migrate-config              # writes owlry.lua; refuses to overwrite
owlry migrate-config --force      # overwrite an existing owlry.lua
```

The migrator is deterministic, emits only values that differ from defaults,
and normalises pre-v2 aliases such as `system` to `power` and `badge_sys` to
`badge_power`.

## Validation and inspection

```bash
owlry config validate   # errors exit 1, warnings exit 2
owlry config show       # resolved effective config as TOML
```

`owlry config validate` parses the active configuration. `owlry config show`
prints the result of merging defaults with the user file.
