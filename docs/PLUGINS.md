# Available Plugins

Owlry's functionality is provided through a modular plugin system. This document describes all available plugins.

## Plugin Categories

### Static Providers

Static providers load their items once at startup (and on manual refresh). They're best for data that doesn't change frequently.

### Dynamic Providers

Dynamic providers evaluate queries in real-time. Each keystroke triggers a new query, making them ideal for calculations, searches, and other interactive features.

### Widget Providers

Widget providers display persistent information at the top of results (weather, media controls, timers).

---

## Core Plugins

### owlry-plugin-calculator

**Type:** Dynamic
**Prefix:** `:calc`, `=`, `calc `
**Package:** `owlry-plugin-calculator`

Evaluate mathematical expressions in real-time.

**Examples:**
```
= 5 + 3           → 8
= sqrt(16)        → 4
= sin(pi/2)       → 1
= 2^10            → 1024
= (1 + 0.05)^12   → 1.7958...
```

**Supported operations:**
- Basic: `+`, `-`, `*`, `/`, `^` (power), `%` (modulo)
- Functions: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`
- Functions: `sqrt`, `abs`, `floor`, `ceil`, `round`
- Functions: `ln`, `log`, `log10`, `exp`
- Constants: `pi`, `e`

---

### owlry-plugin-system

**Type:** Static
**Prefix:** `:sys`
**Package:** `owlry-plugin-system`

System power and session management commands.

**Actions:**
| Name | Description | Command |
|------|-------------|---------|
| Shutdown | Power off | `systemctl poweroff` |
| Reboot | Restart | `systemctl reboot` |
| Reboot into BIOS | UEFI setup | `systemctl reboot --firmware-setup` |
| Suspend | Sleep (RAM) | `systemctl suspend` |
| Hibernate | Sleep (disk) | `systemctl hibernate` |
| Lock Screen | Lock session | `loginctl lock-session` |
| Log Out | End session | `loginctl terminate-session self` |

---

### owlry-plugin-ssh

**Type:** Static
**Prefix:** `:ssh`
**Package:** `owlry-plugin-ssh`

SSH hosts parsed from `~/.ssh/config`.

**Features:**
- Parses `Host` entries from SSH config
- Ignores wildcards (`Host *`)
- Opens connections in your configured terminal

---

### owlry-plugin-clipboard

**Type:** Static
**Prefix:** `:clip`
**Package:** `owlry-plugin-clipboard`
**Dependencies:** `cliphist`, `wl-clipboard`

Clipboard history integration with cliphist.

**Features:**
- Shows last 50 clipboard entries
- Previews text content (truncated to 80 chars)
- Select to copy back to clipboard

---

### owlry-plugin-emoji

**Type:** Static
**Prefix:** `:emoji`
**Package:** `owlry-plugin-emoji`
**Dependencies:** `wl-clipboard`

400+ searchable emoji with keywords.

**Examples:**
```
:emoji heart    → ❤️ 💙 💚 💜 ...
:emoji smile    → 😀 😃 😄 😁 ...
:emoji fire     → 🔥
```

---

### owlry-plugin-scripts

**Type:** Static
**Prefix:** `:script`
**Package:** `owlry-plugin-scripts`

User scripts from `~/.local/share/owlry/scripts/`.

**Setup:**
```bash
mkdir -p ~/.local/share/owlry/scripts
cat > ~/.local/share/owlry/scripts/backup.sh << 'EOF'
#!/bin/bash
rsync -av ~/Documents /backup/
notify-send "Backup complete"
EOF
chmod +x ~/.local/share/owlry/scripts/backup.sh
```

---

### owlry-plugin-bookmarks

**Type:** Static
**Prefix:** `:bm`
**Package:** `owlry-plugin-bookmarks`

Browser bookmarks from Firefox and Chromium-based browsers.

**Supported browsers:**
- Firefox (reads places.sqlite)
- Google Chrome
- Brave
- Microsoft Edge
- Vivaldi
- Chromium

---

### owlry-plugin-websearch

**Type:** Dynamic
**Prefix:** `:web`, `?`, `web `
**Package:** `owlry-plugin-websearch`

Web search with configurable search engine.

**Examples:**
```
? rust programming    → Search for "rust programming"
web linux tips        → Search for "linux tips"
```

**Configuration:**
```toml
[providers]
search_engine = "duckduckgo"  # or: google, bing, startpage
# custom_search_url = "https://search.example.com/?q={}"
```

---

### owlry-plugin-filesearch

**Type:** Dynamic
**Prefix:** `:file`, `/`, `find `
**Package:** `owlry-plugin-filesearch`
**Dependencies:** `fd` (recommended) or `mlocate`

Real-time file search.

**Examples:**
```
/ .bashrc         → Find files matching ".bashrc"
find config       → Find files matching "config"
```

**Configuration:**
```toml
[providers]
file_search_max_results = 50
# file_search_paths = ["/home", "/etc"]  # Custom search paths
```

---

### owlry-plugin-systemd

**Type:** Static (with submenu)
**Prefix:** `:uuctl`
**Package:** `owlry-plugin-systemd`
**Dependencies:** `systemd`

User systemd services with action submenus.

**Features:**
- Lists user services (`systemctl --user`)
- Shows service status (running/stopped/failed)
- Submenu actions: start, stop, restart, enable, disable, status

**Usage:**
1. Search `:uuctl docker`
2. Select a service
3. Choose action from submenu

---

## Widget Plugins

### owlry-plugin-weather

**Type:** Widget (Static)
**Package:** `owlry-plugin-weather`

Current weather displayed at the top of results.

**Supported APIs:**
- wttr.in (default, no API key required)
- OpenWeatherMap (requires API key)
- Open-Meteo (no API key required)

**Note:** Weather configuration is currently embedded in the plugin. Future versions will support runtime configuration.

**Features:**
- Temperature, condition, humidity, wind speed
- Weather icons from Weather Icons font
- 15-minute cache
- Click to open detailed forecast

---

### owlry-plugin-media

**Type:** Widget (Static)
**Package:** `owlry-plugin-media`

MPRIS media player controls.

**Features:**
- Shows currently playing track
- Artist, title, album art
- Play/pause, next, previous controls
- Works with Spotify, Firefox, VLC, etc.

---

### owlry-plugin-pomodoro

**Type:** Widget (Static)
**Package:** `owlry-plugin-pomodoro`

Pomodoro timer with work/break cycles.

**Features:**
- Configurable work session duration
- Configurable break duration
- Session counter
- Desktop notifications on phase completion
- Persistent state across sessions

**Controls:**
- Start/Pause timer
- Skip to next phase
- Reset timer and sessions

---

## Bundle Packages

For convenience, plugins are available in bundle meta-packages:

| Bundle | Plugins |
|--------|---------|
| `owlry-meta-essentials` | calculator, system, ssh, scripts, bookmarks |
| `owlry-meta-widgets` | weather, media, pomodoro |
| `owlry-meta-tools` | clipboard, emoji, websearch, filesearch, systemd |
| `owlry-meta-full` | All of the above |

```bash
# Install everything
yay -S owlry-meta-full

# Or pick a bundle
yay -S owlry-meta-essentials owlry-meta-widgets
```

---

## Runtime Packages

For custom user plugins written in Lua or Rune:

| Package | Description |
|---------|-------------|
| `owlry-lua` | Lua 5.4 runtime for user plugins |
| `owlry-rune` | Rune runtime for user plugins |

User plugins are placed in `~/.config/owlry/plugins/`.

See [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) for creating custom plugins.
